use ratatui::{
    Frame, // Added Wrap for Paragraphs
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style}, // Added Rect for inner areas if needed
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap}, // Added Modifier for more styling options
};

use crate::app::App;
// Assuming App is in crate::app

pub fn ui<B: Backend>(f: &mut Frame, app: &App) {
    // === Layout Definitions ===

    // Main layout: Player (top) and Content (bottom)
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Player top
            Constraint::Min(0),    // Content below
        ])
        .split(f.size());

    let player_chunk = main_chunks[0];
    let content_chunk = main_chunks[1];

    // Content layout: Podcasts | Episodes | Show Notes
    let content_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34), // Use 34 to sum to 100 with two 33s
        ])
        .split(content_chunk);

    let podcasts_chunk = content_columns[0];
    let episodes_chunk = content_columns[1];
    let show_notes_chunk = content_columns[2];

    // === Player Panel ===
    let (player_title, player_text) =
        if let Some((podcast_title, episode_title)) = &app.playing_episode {
            ("Now Playing".to_string(), format!("▶ {} - {}", podcast_title, episode_title))
        } else {
            ("Not Playing".to_string(), " ".to_string()) // Display a space or empty string
        };

    let player_widget = Paragraph::new(player_text)
        .style(Style::default().fg(Color::LightGreen)) // Style for the text
        .wrap(Wrap { trim: true }) // Wrap text if it's too long
        .block(
            Block::default()
                .title(player_title)
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green)), // Style for the block
        );
    f.render_widget(player_widget, player_chunk);

    // === Podcasts Panel (Left) ===
    let podcast_list_items: Vec<ListItem> = app
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let item_style = if Some(i) == app.selected_podcast_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(podcast.title().to_string()).style(item_style) // Ensure title is String or Text
        })
        .collect();

    let podcasts_list_widget = List::new(podcast_list_items)
        .block(
            Block::default()
                .title("Podcasts")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White)),
        )
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)) // Consistent with item_style
        .highlight_symbol(">> "); // Optional: symbol for selected item
    f.render_widget(podcasts_list_widget, podcasts_chunk);

    // === Episodes Panel (Middle) ===
    let episodes_list_widget = if let Some(selected_podcast) = app.selected_podcast() {
        let episode_list_items: Vec<ListItem> = selected_podcast
            .episodes()
            .iter()
            .enumerate()
            .map(|(i, episode)| {
                let item_style = if Some(i) == app.selected_episode_index {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(episode.title().to_string()).style(item_style)
            })
            .collect();

        List::new(episode_list_items)
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ")
    } else {
        // Display placeholder if no podcast is selected
        List::new(vec![ListItem::new("No podcast selected")])
    };

    f.render_widget(
        episodes_list_widget.block(
            // Apply the block to the conditionally created List
            Block::default()
                .title("Episodes")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White)),
        ),
        episodes_chunk,
    );

    // === Show Notes Panel (Right) ===
    let show_notes_text = if let Some(episode) = app.selected_episode() {
        // Assuming Episode has a description method that returns Option<&str>
        // And that description contains the show notes (might need HTML stripping/formatting)
        episode.description().unwrap_or("No show notes available.").to_string()
    } else {
        "Select an episode to see show notes.".to_string()
    };

    let show_notes_widget = Paragraph::new(show_notes_text)
        .wrap(Wrap { trim: true }) // Important for long text
        .block(
            Block::default()
                .title("Show Notes")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White)),
        );
    f.render_widget(show_notes_widget, show_notes_chunk);
}
