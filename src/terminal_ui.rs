// src/terminal_ui.rs
use std::rc::Rc;
use crate::app::{App, FocusedPanel, PlaybackStatus};
use log::error;
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn format_episode_description(description: Option<&str>) -> String {
    const DEFAULT_TEXT_WIDTH: usize = 80;
    match description {
        Some(desc_str) => {
            if desc_str.contains('<') && desc_str.contains('>') && desc_str.contains("</") {
                match html2text::from_read(desc_str.as_bytes(), DEFAULT_TEXT_WIDTH) {
                    Ok(text_content) => {
                        text_content
                            .lines()
                            .map(|line| line.trim_end())
                            .filter(|line| !line.is_empty())
                            .collect::<Vec<&str>>()
                            .join("\n")
                    }
                    Err(_e) => {
                        error!("Failed to parse HTML description with html2text: {}", _e);
                        desc_str.to_string()
                    }
                }
            } else {
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
    let main_chunks: Rc<[Rect]> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(frame_size);

    let content_chunk = main_chunks[1];

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
    let layout_chunks = compute_layout(frame_size);

    let is_show_notes_focused = app.focused_panel == FocusedPanel::ShowNotes;
    let focused_style = Style::default().fg(Color::Cyan);
    let default_style = Style::default().fg(Color::White);

    let temp_show_notes_block = Block::default()
        .title("Show Notes Placeholder")
        .borders(Borders::ALL)
        .border_style(if is_show_notes_focused { focused_style } else { default_style });

    let inner_area = temp_show_notes_block.inner(layout_chunks.show_notes_chunk);
    app.show_notes_state.set_dimensions(inner_area.width, inner_area.height);
}

pub fn ui<B: Backend>(f: &mut Frame, app: &mut App) {
    let layout_chunks = compute_layout(f.size());

    let default_style = Style::default().fg(Color::White);
    let focused_style = Style::default().fg(Color::Cyan);
    let selected_item_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let unfocused_selected_item_style = Style::default().fg(Color::LightCyan);

    // --- Player Panel ---
    let player_status_indicator = match app.player_status {
        PlaybackStatus::Playing => "▶️ Playing",
        PlaybackStatus::Paused => "⏸️ Paused",
        PlaybackStatus::Stopped => "⏹️ Stopped",
        PlaybackStatus::Buffering => "🔄 Buffering",
        PlaybackStatus::Error => "⚠️ Error",
    };
    
    let (player_panel_title, player_panel_text) = if let Some((podcast_title, episode_title)) = &app.current_player_episode {
        let current_pos_str = format!("{:02}:{:02}", app.current_playback_progress.as_secs() / 60, app.current_playback_progress.as_secs() % 60);
        let total_duration_str = app.total_playback_duration.map_or("??:??".to_string(), |d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60));
        let title = "Now Playing".to_string();
        let text = format!("{} - {} | {} / {} ({})", podcast_title, episode_title, current_pos_str, total_duration_str, player_status_indicator);
        (title, text)
    } else {
        ("Player".to_string(), format!("{} (No episode selected)", player_status_indicator))
    };
    
    let player_widget = Paragraph::new(player_panel_text)
        .wrap(Wrap { trim: true })
        .block(Block::default().title(player_panel_title).borders(Borders::ALL).style(Style::default().fg(Color::Green)));
    f.render_widget(player_widget, layout_chunks.player_chunk);

    // --- Podcasts Panel ---
    let is_podcasts_panel_focused = app.focused_panel == FocusedPanel::Podcasts;
    let podcasts_list_items: Vec<ListItem> = app.podcasts.iter().enumerate().map(|(i, podcast)| {
        let mut item = ListItem::new(podcast.title().to_string());
        if Some(i) == app.selected_podcast_index {
            item = item.style(if is_podcasts_panel_focused { selected_item_style } else { unfocused_selected_item_style });
        }
        item
    }).collect();
    
    let podcasts_list_widget = List::new(podcasts_list_items)
        .block(Block::default().title("Podcasts").borders(Borders::ALL).border_style(if is_podcasts_panel_focused { focused_style } else { default_style }))
        .highlight_symbol(if is_podcasts_panel_focused { ">> " } else { "   " });
    f.render_widget(podcasts_list_widget, layout_chunks.podcasts_chunk);

    // --- Episodes Panel ---
    let is_episodes_panel_focused = app.focused_panel == FocusedPanel::Episodes;
    let (episodes_panel_title, episodes_list_items) = match app.selected_podcast() {
        Some(podcast) => {
            let title = format!("Episodes for '{}'", podcast.title());
            let items: Vec<ListItem> = podcast.episodes().iter().enumerate().map(|(i, episode)| {
                let mut item = ListItem::new(episode.title().to_string());
                // --- THIS IS THE CRITICAL FIX ---
                // We now check against the ListState's selected index.
                if Some(i) == app.episodes_list_ui_state.selected() {
                    item = item.style(if is_episodes_panel_focused { selected_item_style } else { unfocused_selected_item_style });
                }
                item
            }).collect();
            (title, items)
        }
        None => ("Episodes".to_string(), vec![ListItem::new("Select a podcast to see episodes")]),
    };

    let episodes_list_widget = List::new(episodes_list_items)
        .block(Block::default().title(episodes_panel_title).borders(Borders::ALL).border_style(if is_episodes_panel_focused { focused_style } else { default_style }))
        .highlight_symbol(if is_episodes_panel_focused { ">> " } else { "   " });
    f.render_stateful_widget(episodes_list_widget, layout_chunks.episodes_chunk, &mut app.episodes_list_ui_state);

    // --- Show Notes Panel ---
    let is_show_notes_focused = app.focused_panel == FocusedPanel::ShowNotes;
    let show_notes_title = app.selected_episode().map_or("Show Notes".to_string(), |e| format!("Show Notes: {}", e.title()));
    let show_notes_content = app.show_notes_state.content.clone();
    
    let show_notes_widget = Paragraph::new(show_notes_content)
        .wrap(Wrap { trim: true })
        .block(Block::default().title(show_notes_title).borders(Borders::ALL).border_style(if is_show_notes_focused { focused_style } else { default_style }))
        .scroll((app.show_notes_state.scroll_offset_vertical, 0));
    f.render_widget(show_notes_widget, layout_chunks.show_notes_chunk);

    // --- Hint Bar ---
    let hint_text = "[←/→/Tab] Switch Panel | [↑/↓] Navigate | [Enter] Play | [Space] Pause | [Q] Quit";
    let hint_widget = Paragraph::new(hint_text).style(Style::default().fg(Color::DarkGray)).alignment(Alignment::Center);
    f.render_widget(hint_widget, layout_chunks.hint_chunk);
}