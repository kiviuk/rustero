use crate::podcast::{Episode, Podcast, PodcastURL};
use crate::ui::format_description;
use crate::widgets::scrollable_paragraph::ScrollableParagraphState;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::Backend};
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
    //pub show_notes_scroll: u16, // Stores the vertical scroll offset (number of lines)
    pub show_notes_state: ScrollableParagraphState,
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
            show_notes_state: ScrollableParagraphState::default(),
        };

        app.select_initial_items();

        app
    }

    // =================================== Update show notes =======================================

    // Method to update show notes content AND reset scroll
    // This should be called whenever the selected episode changes.
    fn update_show_notes_content(&mut self) {
        let new_content = if let Some(episode) = self.selected_episode() {
            format_description(episode.description())
        } else if self.selected_podcast().is_some() {
            "Select an episode to see its show notes.".to_string()
        } else {
            "Select a podcast and then an episode to see show notes.".to_string()
        };
        // CRITICAL: Update content after initial selection
        self.show_notes_state.set_content(new_content);
    }

    // ============================== Method to scroll show notes ==================================

    // Methods in App now modify show_notes_state directly
    // These are called by on_key when ShowNotes is focused
    pub fn scroll_show_notes_up_action(&mut self) {
        // Renamed to avoid conflict if methods added to state struct
        self.show_notes_state.scroll_up(1);
    }
    pub fn scroll_show_notes_down_action(&mut self) {
        // self.show_notes_state.calculate_max_scroll(show_notes_chunk_height)
        self.show_notes_state.scroll_down(1);
    }
    pub fn page_up_show_notes_action(&mut self) {
        self.show_notes_state.scroll_up(5); // Or a calculated page size
    }
    pub fn page_down_show_notes_action(&mut self) {
        self.show_notes_state.scroll_down(5); // Or a calculated page size
    }

    // Handle initial Podcast, Episode and Show Notes selection
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
        // Set initial focus if not already default, already default, but could be explicit
        self.focused_panel = FocusedPanel::Podcasts;
        // CRITICAL: Update content after initial selection
        self.update_show_notes_content();
    }

    // Method to add a podcast (e.g., after download)
    // When adding the very first podcast, you want to select it.
    pub fn add_podcast(&mut self, podcast: Podcast) {
        let was_empty = self.podcasts.is_empty();
        self.podcasts.push(podcast);
        if was_empty {
            // Reselect, which will pick the new first one
            self.select_initial_items(); // This will call update_show_notes_content
        }
    }

    // --- Navigation methods for focused panel ---
    pub fn focus_next_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Podcasts => FocusedPanel::Episodes,
            FocusedPanel::Episodes => FocusedPanel::ShowNotes,
            FocusedPanel::ShowNotes => FocusedPanel::Podcasts, // Cycle back
        };
        // If focus changed *to* ShowNotes or *from* ShowNotes, its content might not need to change
        // unless the selected episode also changed. Content is primarily tied to episode selection.
        // Scroll position of ShowNotes might be preserved or reset based on UX preference.
        // For now, scroll is preserved unless episode changes.
    }

    pub fn focus_prev_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Podcasts => FocusedPanel::ShowNotes, // Cycle back
            FocusedPanel::Episodes => FocusedPanel::Podcasts,
            FocusedPanel::ShowNotes => FocusedPanel::Episodes,
        };
        // Similar logic for scroll as focus_next_panel
    }

    // ============================= Scrolling within the focused panel list =======================
    // =================================== Scrolling PODCASTs ======================================
    pub fn select_next_podcast(&mut self) {
        if self.podcasts.is_empty() {
            self.selected_podcast_index = None; // Clear selection if empty
            self.selected_episode_index = None;
            // CRITICAL: Update content after initial selection
            self.update_show_notes_content(); // Update show notes (will show placeholder)
            return;
        }
        let new_index = self.selected_podcast_index.map_or(0, |i| (i + 1) % self.podcasts.len());
        self.selected_podcast_index = Some(new_index);
        self.selected_episode_index = None; // Reset episode selection for new podcast

        // Auto-select first episode of the newly selected podcast
        if let Some(podcast) = self.selected_podcast() {
            if !podcast.episodes().is_empty() {
                self.selected_episode_index = Some(0);
            }
        }
        self.update_show_notes_content(); // Update content and reset scroll for new podcast/episode
    }

    pub fn select_prev_podcast(&mut self) {
        if self.podcasts.is_empty() {
            self.selected_podcast_index = None;
            self.selected_episode_index = None;
            self.update_show_notes_content();
            return;
        }
        let len = self.podcasts.len();
        let new_index = self.selected_podcast_index.map_or(len - 1, |i| (i + len - 1) % len);
        self.selected_podcast_index = Some(new_index);
        self.selected_episode_index = None;

        if let Some(podcast) = self.selected_podcast() {
            if !podcast.episodes().is_empty() {
                self.selected_episode_index = Some(0);
            }
        }
        self.update_show_notes_content();
    }

    // ==================================== Scrolling EPISODEs =====================================
    pub fn select_next_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            let episodes = podcast.episodes();
            if episodes.is_empty() {
                self.selected_episode_index = None;
                self.update_show_notes_content(); // Update to "no episodes" message
                return;
            }
            let new_index = self.selected_episode_index.map_or(0, |i| (i + 1) % episodes.len());
            self.selected_episode_index = Some(new_index);
            self.update_show_notes_content(); // Update content and reset scroll for new episode
        } else {
            // No podcast selected, ensure episode index is None
            if self.selected_episode_index.is_some() {
                self.selected_episode_index = None;
                self.update_show_notes_content();
            }
        }
    }

    pub fn select_prev_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            let episodes = podcast.episodes();
            if episodes.is_empty() {
                self.selected_episode_index = None;
                self.update_show_notes_content();
                return;
            }
            let len = episodes.len();
            let new_index = self.selected_episode_index.map_or(len - 1, |i| (i + len - 1) % len);
            self.selected_episode_index = Some(new_index);
            self.update_show_notes_content();
        } else {
            if self.selected_episode_index.is_some() {
                self.selected_episode_index = None;
                self.update_show_notes_content();
            }
        }
    }

    pub fn select_next_item_in_focused_list(&mut self) {
        match self.focused_panel {
            FocusedPanel::Podcasts => self.select_next_podcast(),
            FocusedPanel::Episodes => self.select_next_episode(),
            FocusedPanel::ShowNotes => {}
        }
    }

    pub fn select_prev_item_in_focused_list(&mut self) {
        match self.focused_panel {
            FocusedPanel::Podcasts => self.select_prev_podcast(),
            FocusedPanel::Episodes => self.select_prev_episode(),
            FocusedPanel::ShowNotes => { /* ... */ }
        }
    }

    // --- Key Handler ---
    pub fn on_key(&mut self, key: KeyCode) {
        // Handle global quit first
        if key == KeyCode::Char('q') {
            self.should_quit = true;
            return;
        }

        match self.focused_panel {
            FocusedPanel::Podcasts => match key {
                KeyCode::Down => self.select_next_podcast(),
                KeyCode::Up => self.select_prev_podcast(),
                KeyCode::Right | KeyCode::Tab => self.focus_next_panel(),
                KeyCode::Left | KeyCode::BackTab => self.focus_prev_panel(),
                _ => {}
            },
            FocusedPanel::Episodes => match key {
                KeyCode::Down => self.select_next_episode(),
                KeyCode::Up => self.select_prev_episode(),
                KeyCode::Right | KeyCode::Tab => self.focus_next_panel(),
                KeyCode::Left | KeyCode::BackTab => self.focus_prev_panel(),
                // KeyCode::Char(' ') => { /* Play/Pause logic */ }
                _ => {}
            },
            FocusedPanel::ShowNotes => match key {
                KeyCode::Down => self.scroll_show_notes_down_action(),
                KeyCode::Up => self.scroll_show_notes_up_action(),
                KeyCode::PageDown => self.page_down_show_notes_action(),
                KeyCode::PageUp => self.page_up_show_notes_action(),
                KeyCode::Right | KeyCode::Tab => self.focus_next_panel(),
                KeyCode::Left | KeyCode::BackTab => self.focus_prev_panel(),
                _ => {}
            },
        }
    }

    // --- Getters for selected items (no changes needed here from before) ---
    pub fn selected_podcast(&self) -> Option<&Podcast> {
        self.selected_podcast_index.and_then(|i| self.podcasts.get(i))
    }

    pub fn selected_episode(&self) -> Option<&Episode> {
        self.selected_podcast()
            .and_then(|p| self.selected_episode_index.and_then(|i| p.episodes().get(i)))
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

    let res = run_app_loop(&mut terminal, &mut app);

    // Restore the terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

pub fn run_app_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        let frame_size = terminal.get_frame().size(); // Fetch once before drawing
        crate::ui::prepare_ui_layout(app, frame_size);
        terminal.draw(|f| crate::ui::ui::<B>(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            // Poll with timeout
            if let Event::Key(key_event) = event::read()? {
                // key_event not just key
                app.on_key(key_event.code);
            }
        }

        // Add a small sleep here if your app does no other async work in this loop,
        // to yield CPU. If other async tasks are spawned, Tokio handles yielding.
        std::thread::sleep(std::time::Duration::from_millis(10)); // Example, if purely sync loop
    }

    Ok(())
}
