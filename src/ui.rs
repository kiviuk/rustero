use crate::app::{App, FocusedPanel};
use ratatui::{
    Frame,
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect}, // Added Rect for inner areas if needed
    style::{Color, Modifier, Style},               // Added Modifier for more styling options
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap}, // Added Wrap for Paragraphs
};

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
    //
    // eprintln!("--- UI Frame ---");
    // eprintln!("Show Notes Chunk: {:?}", layout_chunks.show_notes_chunk);
    // eprintln!("Inner Area for Paragraph: {:?}", inner_area);
    // eprintln!("Set state dimensions to: width={}, height={}", inner_area.width, inner_area.height);
    // eprintln!("App Show Notes State - panel_w: {}, panel_h: {}", app.show_notes_state.panel_width, app.show_notes_state.panel_height);
    // eprintln!("App Show Notes State - content lines: {}", app.show_notes_state.content.lines().count());
    // eprintln!("App Show Notes State - max_scroll: {}", app.show_notes_state.max_scroll_vertical()); // Call internal directly for debug
    // eprintln!("App Show Notes State - current_scroll: {}", app.show_notes_state.scroll_offset_vertical);

    app.show_notes_state.set_dimensions(inner_area.width, inner_area.height);
}

pub fn ui<B: Backend>(f: &mut Frame, app: &App) {
    // === Layout Definitions ===

    let layout = compute_layout(f.size());

    let player_chunk = layout.player_chunk;
    // let content_chunk = layout.content_chunk;
    let hint_chunk = layout.hint_chunk;

    let podcasts_chunk = layout.podcasts_chunk;
    let episodes_chunk = layout.episodes_chunk;
    let show_notes_chunk = layout.show_notes_chunk;

    // === Define Styles ===
    let default_style = Style::default().fg(Color::White);
    let focused_style = Style::default().fg(Color::Cyan); // Or another distinct color like LightBlue
    let selected_item_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let unfocused_selected_item_style = Style::default().fg(Color::LightCyan); // If you want to dim selection in unfocused lists

    let selected_podcast = app.selected_podcast();
    let selected_episode = app.selected_episode();

    // === Player Panel ===
    let (player_title, player_text) =
        if let Some((podcast_title, episode_title)) = &app.playing_episode {
            ("Now Playing".to_string(), format!("▶ {} - {}", podcast_title, episode_title))
        } else {
            ("Not Playing".to_string(), " ".to_string()) // Display a space or empty string
        };

    let player_widget = Paragraph::new(player_text)
        // .style(Style::default().fg(Color::LightGreen)) // Style for the text
        .wrap(Wrap { trim: true }) // Wrap text if it's too long
        .block(
            Block::default()
                .title(player_title)
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green)), // Style for the block
        );
    f.render_widget(player_widget, player_chunk);

    // =============================================================================================
    // ================================== Podcasts Panel (Left) ===================================
    let is_podcasts_focused = app.focused_panel == FocusedPanel::Podcasts;
    let podcasts_block = Block::default() // Create the block styling separately
        .title("Podcasts")
        .borders(Borders::ALL)
        .border_style(if is_podcasts_focused { focused_style } else { default_style });

    let podcast_list_items: Vec<ListItem> = app
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let mut item = ListItem::new(podcast.title().to_string());
            if Some(i) == app.selected_podcast_index {
                if is_podcasts_focused {
                    item = item.style(selected_item_style); // Use the global Yellow Bold
                } else {
                    item = item.style(unfocused_selected_item_style); // Use the global DarkGray
                }
            } else {
                item = item.style(default_style); // Non-selected items
            }
            item
        })
        .collect();

    let podcasts_list_widget = List::new(podcast_list_items)
        .block(podcasts_block) // Pass the pre-styled block
        .highlight_symbol(if is_podcasts_focused { ">> " } else { "   " }); // Keep this conditional

    f.render_widget(podcasts_list_widget, podcasts_chunk);

    // =============================================================================================
    // ============================== Episodes Panel (Middle) ======================================
    let is_episode_focused = app.focused_panel == FocusedPanel::Episodes;

    // 1. Prepare List Items or Placeholder Message for Episodes
    let episode_list_items: Vec<ListItem> = match selected_podcast {
        Some(selected_podcast) => {
            if selected_podcast.episodes().is_empty() {
                vec![ListItem::new("No episodes for this podcast").style(default_style)]
            } else {
                selected_podcast
                    .episodes()
                    .iter()
                    .enumerate()
                    .map(|(i, episode)| {
                        let mut item = ListItem::new(episode.title().to_string());
                        if Some(i) == app.selected_episode_index {
                            item = item.style(if is_episode_focused {
                                selected_item_style
                            } else {
                                unfocused_selected_item_style
                            });
                        } else {
                            item = item.style(default_style); // Non-selected items
                        }
                        item
                    })
                    .collect()
            }
        }
        None => {
            vec![ListItem::new("Select a podcast to see episodes").style(default_style)]
        }
    };

    // 2. Prepare the Block for the Episodes Panel
    let episode_panel_title = selected_podcast.map_or_else(
        || "Episodes".to_string(), // Default title if no podcast selected
        |p| format!("Episodes for '{}'", p.title()), // Title with podcast name
    );

    let episodes_block = Block::default()
        .title(episode_panel_title) // Use the determined title
        .borders(Borders::ALL)
        .border_style(if is_episode_focused { focused_style } else { default_style });

    // 3. Construct the List Widget
    let episodes_list_widget = List::new(episode_list_items)
        .block(episodes_block)
        .highlight_symbol(if is_episode_focused { ">> " } else { "   " });
    // .highlight_style(selected_item_style) // Still optional/conditional based on ListState usage

    f.render_widget(episodes_list_widget, episodes_chunk);

    // =============================================================================================
    // ============================== Show Notes Panel (Right) =====================================
    let is_show_notes_focused = app.focused_panel == FocusedPanel::ShowNotes;

    // 1. Prepare Show Notes Text Content
    let show_notes_text_content = app.show_notes_state.content.clone();

    // 2. Prepare the Dynamic Panel Title
    let show_notes_panel_title_string: String = match selected_podcast {
        Some(podcast) => {
            match selected_episode {
                Some(episode) => {
                    // Both podcast and episode are selected
                    format!("Show Notes: {} - {}", podcast.title(), episode.title())
                }
                None => {
                    // Only podcast is selected, no specific episode yet
                    format!("Show Notes for '{}' (Select an episode)", podcast.title())
                }
            }
        }
        None => {
            // No podcast selected
            "Show Notes".to_string()
        }
    };

    // 3. Prepare the Block for the Show Notes Panel
    let show_notes_block = Block::default()
        .title(show_notes_panel_title_string) // Use the dynamically created title string
        .borders(Borders::ALL)
        .border_style(if is_show_notes_focused { focused_style } else { default_style });

    // 4. Construct the Paragraph Widget
    let show_notes_widget = Paragraph::new(show_notes_text_content)
        .wrap(Wrap { trim: true })
        .style(default_style) // Assuming default_style for the text
        .block(show_notes_block)
        .scroll((app.show_notes_state.scroll_offset_vertical, 0));

    // 5. Render
    f.render_widget(show_notes_widget, show_notes_chunk);

    // =============================================================================================
    // =============================== Hint Bar Panel (Bottom) =====================================

    // === Hint Bar / Status Bar (Bottom) ===
    let hint_text = "[←/→/Tab] Switch Panel | [↑/↓] Navigate List | [Space] Play/Pause | [Q] Quit";
    // You can make this dynamic later if keybindings change based on context

    let hint_widget = Paragraph::new(hint_text)
        .style(Style::default().fg(Color::DarkGray)) // Subtle color for hints
        .alignment(ratatui::layout::Alignment::Center); // Optional: center the text

    // If you want borders around the hint bar (Constraint::Length(3) for main_chunks[2] then):
    // let hint_widget = Paragraph::new(hint_text)
    //     .style(Style::default().fg(Color::DarkGray))
    //     .alignment(ratatui::layout::Alignment::Center)
    //     .block(Block::default().borders(Borders::TOP)); // Only top border

    f.render_widget(hint_widget, hint_chunk);
}
