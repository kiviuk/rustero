use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::Backend,
    Terminal,
};

use std::io;
use crate::podcast::{Podcast, Episode, PodcastURL};

pub struct App {
    pub should_quit: bool,
    pub podcasts: Vec<Podcast>,
    pub selected_podcast_index: Option<usize>,
    pub selected_episode_index: Option<usize>,
    pub playing_episode: Option<(String, String)>, // (podcast title, episode title)
}


impl App {
    pub fn new() -> App {
        App {
            should_quit: false,
            podcasts: Vec::new(),
            selected_podcast_index: None,
            selected_episode_index: None,
            playing_episode: None,
        }
    }
    
    // Add simple navigation methods
    pub fn select_next_podcast(&mut self) {
        if self.podcasts.is_empty() { return; }
        self.selected_podcast_index = Some(match self.selected_podcast_index {
            Some(i) if i + 1 < self.podcasts.len() => i + 1,
            _ => 0,
        });
        self.selected_episode_index = None; // Reset episode selection
    }

    pub fn select_prev_podcast(&mut self) {
        if self.podcasts.is_empty() { return; }
        self.selected_podcast_index = Some(match self.selected_podcast_index {
            Some(i) if i > 0 => i - 1,
            _ => self.podcasts.len() - 1,
        });
        self.selected_episode_index = None; // Reset episode selection
    }

    pub fn selected_podcast(&self) -> Option<&Podcast> {
        self.selected_podcast_index.map(|i| &self.podcasts[i])
    }

    pub fn selected_episode(&self) -> Option<&Episode> {
        self.selected_podcast()
            .and_then(|p| self.selected_episode_index.map(|i| &p.episodes()[i]))
    }

    pub fn on_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down => self.select_next_podcast(),
            KeyCode::Up => self.select_prev_podcast(),
            // Add more key handlers as needed
            _ => {}
        }
    }

    pub fn load_test_podcast(&mut self) {
        // Create a test podcast with some episodes
        let test_podcast = Podcast::new(
            PodcastURL::new("http://example.com/feed"),
            "Test Podcast".to_string(),
            Some("A test podcast".to_string()),
            None,
            None,
            vec![], // We can add test episodes here if needed
        );
        self.podcasts.push(test_podcast);
    }


}

pub fn start_ui(initial_app: Option<App>) -> Result<()> {
    // Set up the terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Use provided app or create a new empty one
    let mut app = initial_app.unwrap_or_else(App::new);

    let res = run_app(&mut terminal, &mut app);

    // Restore the terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {}", e);
    }

    Ok(())
}


pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| crate::ui::ui::<B>(f, app))?;

        if let Event::Key(key) = event::read()? {
            app.on_key(key.code);
        }
    }

    Ok(())
}