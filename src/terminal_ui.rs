// src/terminal_ui
use crate::app::{App, FocusedPanel};
use ratatui::{
    Frame,
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect}, // Added Rect for inner areas if needed
    style::{Color, Modifier, Style},               // Added Modifier for more styling options
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap}, // Added Wrap for Paragraphs
};
use crate::podcast::{Episode, Podcast};

/// Formats a given description string by either processing it as HTML, plain text,
/// or returning a default message if the description is `None`.
///
/// # Arguments
///
/// * `description` - An `Option<&str>` containing the description text.
///   - If `Some`, the string may contain plain text or HTML.
///   - If `None`, a default "No show notes available" message is returned.
///
/// # Returns
///
/// A `String` containing the formatted description:
/// - If the description appears to contain HTML tags (determined by a simple heuristic),
///   the function attempts to parse and extract plain text using the `html2text` crate.
///   - Successfully parsed text is further processed to:
///     - Trim trailing whitespace on each line.
///     - Remove any empty lines.
/// - If HTML parsing fails, the raw input string is returned as a fallback,
///   and an error is logged to `stderr`.
/// - If the description does not contain HTML or parsing is not required, the plain `&str`
///   is directly converted to a `String`.
/// - If `description` is `None`, a default string "No show notes available for this episode."
///   is returned.
pub fn format_description(description: Option<&str>) -> String {
    // The trim() is redundant here as the to_string() will already trim whitespace.
    // Also, consider returning Cow<'a, str> to avoid unnecessary allocations when the input is already a String.
    const DEFAULT_TEXT_WIDTH: usize = 80;
    match description {
        Some(desc_str) => {
            // A simple heuristic: if it looks like HTML, try to convert it.
            if desc_str.contains('<') && desc_str.contains('>') && desc_str.contains("</") {
                // Slightly better HTML check
                match html2text::from_read(desc_str.as_bytes(), DEFAULT_TEXT_WIDTH) {
                    // 80 is example width
                    Ok(text_content) => {
                        // Process the successfully converted text
                        text_content
                            .lines()
                            .map(|line| line.trim_end()) // Trim trailing whitespace
                            .filter(|line| !line.is_empty()) // Optional: remove empty lines
                            .collect::<Vec<&str>>() // Collect as Vec<&str> first
                            .join("\n")
                    }
                    Err(_e) => {
                        // If html2text fails, fallback to rendering the original string
                        // You might want to log the error _e here for debugging
                        eprintln!("Failed to parse HTML description with html2text: {}", _e);
                        desc_str.to_string() // Fallback
                    }
                }
            } else {
                // Assume it's plain text or Markdown that we're not yet processing richly
                desc_str.to_string()
            }
        }
        None => "No show notes available for this episode.".to_string(),
    }
    .trim()
    .to_string()
}

pub struct LayoutChunks {
    pub player_chunk: Rect,
    pub content_chunk: Rect,
    pub hint_chunk: Rect,
    pub podcasts_chunk: Rect,
    pub episodes_chunk: Rect,
    pub show_notes_chunk: Rect,
}

pub fn compute_layout(frame_size: Rect) -> LayoutChunks {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(frame_size);

    let content_chunk = main_chunks[1];

    let content_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(content_chunk);

    LayoutChunks {
        player_chunk: main_chunks[0],
        content_chunk,
        hint_chunk: main_chunks[2],
        podcasts_chunk: content_columns[0],
        episodes_chunk: content_columns[1],
        show_notes_chunk: content_columns[2],
    }
}

/// This function prepares layout (only for show_notes height right now)
/// and updates mutable state outside the draw closure.
pub fn prepare_ui_layout(app: &mut App, frame_size: Rect) {
    let layout_chunks = compute_layout(frame_size);

    let is_show_notes_focused = app.focused_panel == FocusedPanel::ShowNotes; // Need app state for focus style
    let focused_style = Style::default().fg(Color::Cyan); // Assuming these are accessible or defined
    let default_style = Style::default().fg(Color::White);

    // Temporarily construct the block to get its inner dimensions.
    // The title string here doesn't have to be the final dynamic one,
    // as long as it doesn't change the *height* of the title area.
    // If the title string can wrap and take multiple lines, this becomes more complex.
    // Assuming single-line titles for now for simplicity of inner calculation.
    let temp_show_notes_block = Block::default()
        .title("Show Notes Placeholder") // Placeholder or actual title logic
        .borders(Borders::ALL)
        .border_style(if is_show_notes_focused { focused_style } else { default_style });

    // 2. Calculate the inner area of this block IF IT WERE RENDERED in show_notes_chunk.
    let inner_area = temp_show_notes_block.inner(layout_chunks.show_notes_chunk);

    app.show_notes_state.set_dimensions(inner_area.width, inner_area.height);
}

pub fn ui<B: Backend>(f: &mut Frame, app: &mut App) {
    // === Layout Definitions ===
    let layout_chunks: LayoutChunks = compute_layout(f.size());
    let player_chunk: Rect = layout_chunks.player_chunk;
    let hint_chunk: Rect = layout_chunks.hint_chunk;
    let podcasts_chunk: Rect = layout_chunks.podcasts_chunk;
    let episodes_chunk: Rect = layout_chunks.episodes_chunk;
    let show_notes_chunk: Rect = layout_chunks.show_notes_chunk;

    // === Define Styles ===
    let default_style: Style = Style::default().fg(Color::White);
    let focused_style: Style = Style::default().fg(Color::Cyan); // Or another distinct color like LightBlue
    let selected_item_style: Style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let unfocused_selected_item_style: Style = Style::default().fg(Color::LightCyan); // If you want to dim selection in unfocused lists

    // --- Gather data that depends on immutable borrows of `app` first ---

    // Data for Player Panel
    let (player_panel_title, player_panel_text): (String, String) =
        if let Some((podcast_title, episode_title)) = &app.playing_episode { // Immutable borrow
            ("Now Playing".to_string(), format!("▶ {} - {}", podcast_title, episode_title))
        } else {
            ("Not Playing".to_string(), " ".to_string())
        };

    // Data for Podcasts Panel
    let is_podcasts_panel_focused = app.focused_panel == FocusedPanel::Podcasts; // Immutable borrow
    let podcasts_list_items: Vec<ListItem> = app // Immutable borrow
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let mut item = ListItem::new(podcast.title().to_string());
            if Some(i) == app.selected_podcast_index { // Immutable borrow
                item = item.style(if is_podcasts_panel_focused { selected_item_style } else { unfocused_selected_item_style });
            } else {
                item = item.style(default_style);
            }
            item
        })
        .collect();

    // Data for Episodes Panel
    let is_episodes_panel_focused = app.focused_panel == FocusedPanel::Episodes; // Immutable borrow
    let episodes_panel_title: String;
    let episodes_list_items: Vec<ListItem>;

    match app.selected_podcast() { // Immutable borrow
        Some(selected_podcast_ref) => {
            episodes_panel_title = format!("Episodes for '{}'", selected_podcast_ref.title());
            if selected_podcast_ref.episodes().is_empty() {
                episodes_list_items = vec![ListItem::new("No episodes for this podcast").style(default_style)];
            } else {
                episodes_list_items = selected_podcast_ref
                    .episodes()
                    .iter()
                    .enumerate() // We need the index for manual selection styling
                    .map(|(i, episode)| {
                        let mut item = ListItem::new(episode.title().to_string());
                        // Style based on logical selection and panel focus
                        if Some(i) == app.selected_episode_index { // Immutable borrow
                            item = item.style(if is_episodes_panel_focused {
                                selected_item_style
                            } else {
                                unfocused_selected_item_style
                            });
                        } else {
                            item = item.style(default_style);
                        }
                        item
                    })
                    .collect();
            }
        }
        None => {
            episodes_panel_title = "Episodes".to_string();
            episodes_list_items = vec![ListItem::new("Select a podcast to see episodes").style(default_style)];
        }
    }

    // Data for Show Notes Panel
    let is_show_notes_panel_focused: bool = app.focused_panel == FocusedPanel::ShowNotes; // Immutable borrow
    let show_notes_content: String = app.show_notes_state.content.clone(); // Clone the content string
    let show_notes_title: String = { // Use a block to scope borrows for title construction
        let current_podcast_title = app.selected_podcast().map(|p| p.title().to_string()); // Immutable borrow
        let current_episode_title = app.selected_episode().map(|e| e.title().to_string()); // Immutable borrow
        match (current_podcast_title, current_episode_title) {
            (Some(p_title), Some(e_title)) => format!("Show Notes: {} - {}", p_title, e_title),
            (Some(p_title), None) => format!("Show Notes for '{}' (Select an episode)", p_title),
            _ => "Show Notes".to_string(),
        }
    };
    // =================================== Player Panel ============================================
    let player_widget = Paragraph::new(player_panel_text)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(player_panel_title)
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green)),
        );
    f.render_widget(player_widget, layout_chunks.player_chunk);

    // ================================== Podcasts Panel (Left) ====================================
    let podcasts_block_widget = Block::default()
        .title("Podcasts")
        .borders(Borders::ALL)
        .border_style(if is_podcasts_panel_focused { focused_style } else { default_style });
    let podcasts_list_render_widget = List::new(podcasts_list_items)
        .block(podcasts_block_widget)
        .highlight_symbol(if is_podcasts_panel_focused { ">> " } else { "   " });
    f.render_widget(podcasts_list_render_widget, layout_chunks.podcasts_chunk);

    // ============================== Episodes Panel (Middle) ======================================
    let episodes_block_widget = Block::default()
        .title(episodes_panel_title)
        .borders(Borders::ALL)
        .border_style(if is_episodes_panel_focused { focused_style } else { default_style });

    let episodes_list_render_widget = List::new(episodes_list_items)
        .block(episodes_block_widget)
        .highlight_symbol(if is_episodes_panel_focused { ">> " } else { "   " });
    f.render_stateful_widget(
        episodes_list_render_widget,
        layout_chunks.episodes_chunk,
        &mut app.episodes_list_state
    );
    // ============================== Show Notes Panel (Right) =====================================
    let show_notes_block_widget = Block::default()
        .title(show_notes_title)
        .borders(Borders::ALL)
        .border_style(if is_show_notes_panel_focused { focused_style } else { default_style });
    let show_notes_render_widget = Paragraph::new(show_notes_content)
        .wrap(Wrap { trim: true })
        .style(default_style)
        .block(show_notes_block_widget)
        .scroll((app.show_notes_state.scroll_offset_vertical, 0));
    f.render_widget(show_notes_render_widget, layout_chunks.show_notes_chunk);

    // =============================== Hint Bar Panel (Bottom) =====================================
    // You can make this dynamic later if keybindings change based on context
    let hint_text = "[←/→/Tab] Switch Panel | [↑/↓] Navigate List | [Space] Play/Pause | [Q] Quit";
    let hint_widget = Paragraph::new(hint_text)
        .style(Style::default().fg(Color::DarkGray)) // Subtle color for hints
        .alignment(ratatui::layout::Alignment::Center); // Optional: center the text
    f.render_widget(hint_widget, hint_chunk);
}
