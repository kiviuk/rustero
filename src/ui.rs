use ratatui::{
    backend::Backend,
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    text::Text, 
};

use crate::app::App;
pub fn ui<B: Backend>(f: &mut Frame, app: &App) {
    // First split into vertical sections (top player and main content)
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Fixed height for player
            Constraint::Min(0),     // Rest of the space for main content
        ])
        .split(f.size());

    // Create the player panel at the top
    let player_panel = Block::default()
        .title("Now Playing")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));
    f.render_widget(player_panel, main_layout[0]);

    // Split the main content area into three equal columns
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),  // Podcasts list
            Constraint::Percentage(33),  // Episodes list
            Constraint::Percentage(34),  // Show notes
        ])
        .split(main_layout[1]);

    // Left panel (Podcast list)
    let podcasts_panel = Block::default()
        .title("Podcasts")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));
    f.render_widget(podcasts_panel, content_chunks[0]);

    // Middle panel (Episodes)
    let episodes_panel = Block::default()
        .title("Episodes")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));
    f.render_widget(episodes_panel, content_chunks[1]);

    // Right panel (Show notes)
    let show_notes_panel = Block::default()
        .title("Show Notes")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));
    f.render_widget(show_notes_panel, content_chunks[2]);

    // Render podcast titles in left panel
    let podcast_titles: Vec<ListItem> = app.podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let style = if Some(i) == app.selected_podcast_index {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            ListItem::new(podcast.title()).style(style)
        })
        .collect();

    let podcasts_list = List::new(podcast_titles)
        .block(Block::default().title("Podcasts").borders(Borders::ALL));
    f.render_widget(podcasts_list, content_chunks[0]);


    // Render episodes if a podcast is selected
    if let Some(podcast) = app.selected_podcast() {
        let episode_titles: Vec<ListItem> = podcast.episodes()
            .iter()
            .enumerate()
            .map(|(i, episode)| {
                let style = if Some(i) == app.selected_episode_index {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                ListItem::new(episode.title()).style(style)
            })
            .collect();

        let episodes_list = List::new(episode_titles)
            .block(Block::default().title("Episodes").borders(Borders::ALL));
        f.render_widget(episodes_list, content_chunks[1]);
    }

    // Show notes panel (right)
    let show_notes = Block::default()
        .title("Show Notes")
        .borders(Borders::ALL);
    f.render_widget(show_notes, content_chunks[2]);

    // Show player info if there's a playing episode
    if let Some((podcast_title, episode_title)) = &app.playing_episode {
        let now_playing = format!("▶ {} - {}", podcast_title, episode_title);
        let player = Paragraph::new(now_playing)
            .block(Block::default().title("Now Playing").borders(Borders::ALL));
        f.render_widget(player, main_layout[0]);
    } else {
        // Show empty player when nothing is playing
        let player = Block::default()
            .title("Not Playing")
            .borders(Borders::ALL);
        f.render_widget(player, main_layout[0]);
    }

}

