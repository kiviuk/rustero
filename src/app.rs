use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::Backend};

use crate::podcast::{Episode, Podcast, PodcastURL};
use std::io;

#[derive(Debug, PartialEq, Eq, Clone, Copy)] // Added Clone, Copy for easier use
pub enum FocusedPanel {
    Podcasts,
    Episodes,
    ShowNotes,
    // Potentially Player in the future if it becomes interactive
}

impl Default for FocusedPanel {
    fn default() -> Self {
        FocusedPanel::Podcasts // Default focus to the podcasts panel
    }
}

pub struct App {
    pub should_quit: bool,
    pub podcasts: Vec<Podcast>,
    pub selected_podcast_index: Option<usize>,
    pub selected_episode_index: Option<usize>,
    pub playing_episode: Option<(String, String)>, // (podcast title, episode title)
    pub focused_panel: FocusedPanel,
    pub show_notes_scroll: u16, // Stores the vertical scroll offset (number of lines)
}

impl App {
    pub fn new() -> App {
        let mut app = App {
            should_quit: false,
            podcasts: Vec::new(), // Initially empty, will be populated
            selected_podcast_index: None,
            selected_episode_index: None,
            playing_episode: None,
            focused_panel: FocusedPanel::default(), // Initialize focused panel
            show_notes_scroll: 0,                   // Initialize scroll to 0
        };

        app.select_initial_items();

        app
    }

    // Method to scroll show notes up
    pub fn scroll_show_notes_up(&mut self) {
        // Decrease scroll, but not below 0
        self.show_notes_scroll = self.show_notes_scroll.saturating_sub(1);
    }

    // Method to scroll show notes down
    // We'll need to know the total number of lines in the show notes
    // to prevent scrolling too far down. This is tricky because the Paragraph
    // widget does its own wrapping based on width.
    // For now, let's allow scrolling down "infinitely" and the Paragraph will just show blank lines.
    // A more advanced solution would calculate max scroll based on content height and panel height.
    pub fn scroll_show_notes_down(&mut self) {
        self.show_notes_scroll = self.show_notes_scroll.saturating_add(1);
    }

    // Handle initial selection
    pub fn select_initial_items(&mut self) {
        if !self.podcasts.is_empty() {
            self.selected_podcast_index = Some(0); // Select the first podcast

            // Optionally, also select the first episode of that podcast
            if let Some(first_podcast) = self.podcasts.first() {
                if !first_podcast.episodes().is_empty() {
                    self.selected_episode_index = Some(0);
                } else {
                    self.selected_episode_index = None;
                }
            }
        } else {
            // No podcasts, so no selection
            self.selected_podcast_index = None;
            self.selected_episode_index = None;
        }
        // et initial focus if not already default, already default, but could be explicit
        self.focused_panel = FocusedPanel::Podcasts;
    }

    // Method to add a podcast (e.g., after download)
    // When adding the *very first* podcast, you might want to select it.
    pub fn add_podcast(&mut self, podcast: Podcast) {
        let is_first_podcast_added = self.podcasts.is_empty();
        self.podcasts.push(podcast);
        if is_first_podcast_added {
            self.select_initial_items(); // Reselect, which will pick the new first one
        }
    }

    // --- Navigation methods for focused panel ---
    pub fn focus_next_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Podcasts => FocusedPanel::Episodes,
            FocusedPanel::Episodes => FocusedPanel::ShowNotes,
            FocusedPanel::ShowNotes => FocusedPanel::Podcasts, // Cycle back
        };
    }

    pub fn focus_prev_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Podcasts => FocusedPanel::ShowNotes, // Cycle back
            FocusedPanel::Episodes => FocusedPanel::Podcasts,
            FocusedPanel::ShowNotes => FocusedPanel::Episodes,
        };
    }

    // --- Scrolling within the focused list ---
    // (select_next_podcast and select_prev_podcast are already good for the Podcasts panel)

    pub fn select_next_item_in_focused_list(&mut self) {
        match self.focused_panel {
            FocusedPanel::Podcasts => self.select_next_podcast(),
            FocusedPanel::Episodes => self.select_next_episode(),
            FocusedPanel::ShowNotes => {
                /* ShowNotes might not be a list, or needs different scroll */
            }
        }
    }

    pub fn select_prev_item_in_focused_list(&mut self) {
        match self.focused_panel {
            FocusedPanel::Podcasts => self.select_prev_podcast(),
            FocusedPanel::Episodes => self.select_prev_episode(),
            FocusedPanel::ShowNotes => { /* ... */ }
        }
    }

    // --- Episode selection methods (new or refined) ---
    pub fn select_next_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            if podcast.episodes().is_empty() {
                self.selected_episode_index = None;
                return;
            }
            let current_index = self.selected_episode_index.unwrap_or(usize::MAX); // Start before first if None
            if current_index < podcast.episodes().len() - 1 {
                self.selected_episode_index =
                    Some(current_index.saturating_add(1).min(podcast.episodes().len() - 1));
            } else if current_index == usize::MAX && !podcast.episodes().is_empty() {
                // handles if was None
                self.selected_episode_index = Some(0);
            } else if !podcast.episodes().is_empty() {
                // Wrap around to top
                self.selected_episode_index = Some(0);
            }
        } else {
            self.selected_episode_index = None;
        }
    }

    pub fn select_prev_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            if podcast.episodes().is_empty() {
                self.selected_episode_index = None;
                return;
            }
            let current_index = self.selected_episode_index.unwrap_or(0); // Start at first if None
            if current_index > 0 {
                self.selected_episode_index = Some(current_index.saturating_sub(1));
            } else if !podcast.episodes().is_empty() {
                // Wrap around to bottom
                self.selected_episode_index = Some(podcast.episodes().len() - 1);
            }
        } else {
            self.selected_episode_index = None;
        }
    }

    // Update on_key to handle new navigation
    pub fn on_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down => self.select_next_item_in_focused_list(),
            KeyCode::Up => self.select_prev_item_in_focused_list(),
            KeyCode::Right | KeyCode::Tab => self.focus_next_panel(), // Tab also cycles forward
            KeyCode::Left | KeyCode::BackTab => self.focus_prev_panel(), // Shift+Tab (BackTab) cycles backward
            // KeyCode::Enter or KeyCode::Char(' ') for selection/action later
            _ => {}
        }
    }

    // Add simple navigation methods
    pub fn select_next_podcast(&mut self) {
        if self.podcasts.is_empty() {
            return;
        }
        self.selected_podcast_index = Some(match self.selected_podcast_index {
            Some(i) if i + 1 < self.podcasts.len() => i + 1,
            _ => 0,
        });
        self.selected_episode_index = None; // Reset episode selection
    }

    pub fn select_prev_podcast(&mut self) {
        if self.podcasts.is_empty() {
            return;
        }
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
        self.selected_podcast().and_then(|p| self.selected_episode_index.map(|i| &p.episodes()[i]))
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
