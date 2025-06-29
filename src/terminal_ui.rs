// src/terminal_ui.rs
use crate::app::{App, FocusedPanel, PlaybackStatus};
use emojis;
use log::error;
use ratatui::{
    Frame,
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;
const DEFAULT_TEXT_WIDTH: usize = 80;
const TITLE_MAX_LENGTH: usize = 40;
const EPISODE_NAME_MAX_LENGTH: usize = 80;
const PODCAST_NAME_MAX_LENGTH: usize = 50;



/// Removes emojis using grapheme segmentation
fn remove_emojis(text: &str) -> String {
    text.graphemes(true).filter(|g| emojis::get(g).is_none()).collect()
}

/// Truncates text by graphemes and adds ellipsis if needed
fn truncate_with_ellipsis(text: &str, max_length: usize) -> String {
    let graphemes: Vec<&str> = text.graphemes(true).collect();

    if graphemes.len() <= max_length {
        return text.to_string();
    }

    let mut truncated: String = graphemes.into_iter().take(max_length.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

/// Sanitizer for panel titles (podcast/episode titles at top of panels)
/// Removes emojis and limits to ~25 characters
fn sanitize_panel_title(text: &str, fallback: Option<&str>) -> String {
    let source_text = if text.trim().is_empty() { fallback.unwrap_or("[Untitled]") } else { text };

    let no_emojis: String = remove_emojis(source_text);
    let trimmed: &str = no_emojis.trim();

    if trimmed.is_empty() {
        "[Untitled]".to_string()
    } else {
        truncate_with_ellipsis(trimmed, TITLE_MAX_LENGTH)
    }
}

/// Sanitizer for episode names in the episode list
/// Removes emojis but allows longer text for readability
fn sanitize_episode_name(text: &str, fallback: Option<&str>) -> String {
    let source_text =
        if text.trim().is_empty() { fallback.unwrap_or("[Untitled Episode]") } else { text };

    let no_emojis: String = remove_emojis(source_text);
    let trimmed: &str = no_emojis.trim();

    if trimmed.is_empty() {
        "[Untitled Episode]".to_string()
    } else {
        truncate_with_ellipsis(trimmed, EPISODE_NAME_MAX_LENGTH)
    }
}

fn sanitize_podcast_name(text: &str, fallback: Option<&str>) -> String {
    let source_text =
        if text.trim().is_empty() { fallback.unwrap_or("[Untitled Podcast]") } else { text };

    let no_emojis: String = remove_emojis(source_text);
    let trimmed: &str = no_emojis.trim();

    if trimmed.is_empty() {
        "[Untitled Podcast]".to_string()
    } else {
        truncate_with_ellipsis(trimmed, PODCAST_NAME_MAX_LENGTH)
    }
}

/// Sanitizer for show notes content
/// Converts HTML to text while preserving basic formatting and readability
fn sanitize_show_notes(html_content: &str) -> String {
    if html_content.trim().is_empty() {
        return "No show notes available for this episode.".to_string();
    }

    // Convert HTML to plain text, preserving basic structure
    let plain_text = match html2text::from_read(html_content.as_bytes(), DEFAULT_TEXT_WIDTH) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!("Failed to parse HTML in show notes: {}", e);
            html_content.to_string()
        }
    };

    // Clean up excessive whitespace while preserving paragraph breaks
    let cleaned_content = plain_text
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    // Remove emojis but preserve the formatting structure
    let no_emojis = remove_emojis(&cleaned_content);

    if no_emojis.trim().is_empty() {
        "No readable content available for this episode.".to_string()
    } else {
        no_emojis
    }
}

/// Legacy function for backward compatibility - now delegates to appropriate sanitizer
fn prepare_text_for_terminal(text: &str, fallback_text: Option<&str>) -> String {
    sanitize_episode_name(text, fallback_text)
}

/// Specialized formatting for episode descriptions - now uses sanitize_show_notes
pub fn format_episode_description(description: Option<&str>) -> String {
    match description {
        Some(desc_str) => sanitize_show_notes(desc_str),
        None => "No show notes available for this episode.".to_string(),
    }
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
    let main_chunks: Rc<[Rect]> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(frame_size);

    let content_chunk: Rect = main_chunks[1];

    let content_columns: Rc<[Rect]> = Layout::default()
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

pub fn prepare_ui_layout(app: &mut App, frame_size: Rect) {
    let layout_chunks: LayoutChunks = compute_layout(frame_size);

    let temp_show_notes_block: Block =
        Block::default().title("Show Notes Placeholder").borders(Borders::ALL);

    let inner_area: Rect = temp_show_notes_block.inner(layout_chunks.show_notes_chunk);
    app.show_notes_state.set_dimensions(inner_area.width, inner_area.height);
}

pub fn ui<B: Backend>(f: &mut Frame, app: &mut App) {
    // log::info!("[UI DRAW] Current focused panel: {:?}", app.focused_panel);

    let layout_chunks: LayoutChunks = compute_layout(f.size());

    let default_style: Style = Style::default().fg(Color::Black);
    let focused_style: Style = Style::default().fg(Color::Cyan);
    let selected_item_style: Style =
        Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD);
    let unfocused_selected_item_style: Style =
        Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD);

    // --- Player Panel ---
    let player_status_indicator: &str = match app.player_status {
        PlaybackStatus::Playing => "▶️ Playing",
        PlaybackStatus::Paused => "⏸️ Paused",
        PlaybackStatus::Stopped => "⏹️ Stopped",
        PlaybackStatus::Buffering => "🔄 Buffering",
        PlaybackStatus::Error => "⚠️ Error",
    };

    let (sanitized_player_panel_title, sanitized_player_panel_text): (String, String) =
        if let Some((podcast_title, episode_title)) = &app.current_player_episode {
            let current_pos_str: String = format!(
                "{:02}:{:02}",
                app.current_playback_progress.as_secs() / 60,
                app.current_playback_progress.as_secs() % 60
            );
            let total_duration_str: String =
                app.total_playback_duration.map_or("??:??".to_string(), |d| {
                    format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
                });
            let title: String = "Now Playing".to_string();
            let text: String = format!(
                "{} - {} | {} / {} ({})",
                sanitize_panel_title(podcast_title, None),
                sanitize_panel_title(episode_title, None),
                current_pos_str,
                total_duration_str,
                player_status_indicator
            );
            (title, text)
        } else {
            ("Player".to_string(), format!("{} (No episode selected)", player_status_indicator))
        };

    let player_widget: Paragraph =
        Paragraph::new(sanitized_player_panel_text).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(sanitized_player_panel_title)
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green)),
        );
    f.render_widget(player_widget, layout_chunks.player_chunk);

    // --- Podcasts Panel ---
    let is_podcasts_panel_focused: bool = app.focused_panel == FocusedPanel::Podcasts;
    let podcasts_list_items: Vec<ListItem> = app
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let sanitized_name: String = sanitize_podcast_name(podcast.title(), None);
            let mut item: ListItem = ListItem::new(sanitized_name);
            if Some(i) == app.selected_podcast_index {
                item = item.style(if is_podcasts_panel_focused {
                    selected_item_style
                } else {
                    unfocused_selected_item_style
                });
            }
            item
        })
        .collect();

    let podcasts_list_widget: List =
        List::new(podcasts_list_items)
            .block(Block::default().title("Podcasts").borders(Borders::ALL).border_style(
                if is_podcasts_panel_focused { focused_style } else { default_style },
            ))
            .highlight_symbol(if is_podcasts_panel_focused { ">> " } else { "   " });

    f.render_widget(podcasts_list_widget, layout_chunks.podcasts_chunk);

    // --- Episodes Panel ---
    let is_episodes_panel_focused: bool = app.focused_panel == FocusedPanel::Episodes;
    let (episodes_panel_title, episodes_list_items): (String, Vec<ListItem>) = match app
        .selected_podcast()
    {
        Some(podcast) => {
            let title: String =
                format!("Episodes: {}", sanitize_panel_title(podcast.title(), None));
            let items: Vec<ListItem> = podcast
                .episodes()
                .iter()
                .enumerate()
                .map(|(i, episode)| {
                    let sanitized_episode_title: String =
                        sanitize_episode_name(episode.title(), None);
                    let mut item: ListItem = ListItem::new(sanitized_episode_title);
                    if Some(i) == app.episodes_list_ui_state.selected() {
                        item = item.style(if is_episodes_panel_focused {
                            selected_item_style
                        } else {
                            unfocused_selected_item_style
                        });
                    }
                    item
                })
                .collect();
            (title, items)
        }
        None => ("Episodes".to_string(), vec![ListItem::new("Select a podcast to see episodes")]),
    };

    let episodes_list_widget: List =
        List::new(episodes_list_items)
            .block(Block::default().title(episodes_panel_title).borders(Borders::ALL).border_style(
                if is_episodes_panel_focused { focused_style } else { default_style },
            ))
            .highlight_symbol(if is_episodes_panel_focused { ">> " } else { "   " });
    f.render_stateful_widget(
        episodes_list_widget,
        layout_chunks.episodes_chunk,
        &mut app.episodes_list_ui_state,
    );

    // --- Show Notes Panel ---
    let is_show_notes_focused: bool = app.focused_panel == FocusedPanel::ShowNotes;
    let sanitized_show_notes_title: String = app
        .selected_episode()
        .map_or("Show Notes".to_string(), |e| format!("Show Notes: {}", sanitize_panel_title(e.title(), None)));
    let show_notes_content: String = app.show_notes_state.content.clone();
    let sanitized_show_notes_content: String = sanitize_show_notes(&show_notes_content);

    let show_notes_widget: Paragraph = Paragraph::new(sanitized_show_notes_content)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(sanitized_show_notes_title)
                .borders(Borders::ALL)
                .border_style(if is_show_notes_focused { focused_style } else { default_style }),
        )
        .scroll((app.show_notes_state.scroll_offset_vertical, 0));
    f.render_widget(show_notes_widget, layout_chunks.show_notes_chunk);

    // --- Hint Bar ---
    let hint_text: &str =
        "[←/→/Tab] Switch Panel | [↑/↓] Navigate | [Enter] Play | [Space] Pause | [Q] Quit";
    let hint_widget: Paragraph = Paragraph::new(hint_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(hint_widget, layout_chunks.hint_chunk);
}
