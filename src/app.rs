// src/app.rs
use crate::commands::podcast_pipeline_interpreter::PODCAST_DATA_DIR;
use crate::event::AppEvent;
use crate::podcast::{Episode, Podcast, PodcastURL};
use crate::terminal_ui::format_episode_description;
use crate::widgets::scrollable_paragraph::ScrollableParagraphState;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::widgets::ListState;
use ratatui::{Terminal, backend::Backend};
use std::path::PathBuf;
use std::{fs, io};
use tokio::sync::broadcast;
use tokio::sync::broadcast::Receiver;

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
    pub selected_episode_index: Option<usize>, // Logical selection
    pub episodes_list_ui_state: ListState,     // UI state including selection and offset
    pub playing_episode: Option<(String, String)>, // (podcast title, episode title)
    pub focused_panel: FocusedPanel,
    pub show_notes_state: ScrollableParagraphState,
    pub event_rx: Receiver<AppEvent>,
    event_channel_closed_reported: bool, // for the "channel closed" message
}

impl App {
    pub fn new(event_rx: Receiver<AppEvent>) -> App {
        let mut app = App {
            should_quit: false,
            podcasts: Vec::new(), // Initially empty, will be populated by events or initial load
            selected_podcast_index: None,
            selected_episode_index: None,
            episodes_list_ui_state: ListState::default(),
            playing_episode: None,
            focused_panel: FocusedPanel::default(), // Initialize focused panel
            show_notes_state: ScrollableParagraphState::default(),
            event_rx,
            event_channel_closed_reported: false, // Initialize the flag
        };

        app.select_first_podcast();

        app
    }

    // =================================== Update podcasts =========================================

    // App calls this method in its loop to process incoming events.
    // This is the crucial method that App will call in its loop to process incoming events.
    // It should be non-blocking if called frequently in the TUI loop.
    pub fn handle_pending_events(&mut self) {
        match self.event_rx.try_recv() {
            Ok(AppEvent::PodcastReadyForApp { podcast, timestamp: _ }) => {
                // Destructure directly
                // println!("[APP] Received PodcastReadyForApp for: {}", podcast.title());
                self.add_podcast(podcast);
            }
            Err(broadcast::error::TryRecvError::Empty) => { /* No event, normal */ }
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                eprintln!("[APP] Event receiver lagged by {} messages!", n);
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                if !self.event_channel_closed_reported {
                    // println!("[APP] Event channel closed (no senders currently active).");
                    self.event_channel_closed_reported = true;
                }
            }
        }
    }

    // ================================= Method to add a podcast ==================================
    // Method to add a podcast (e.g., after download or for initial setup)
    // This method is now central to updating state when a new podcast arrives.
    pub fn add_podcast(&mut self, podcast: Podcast) {
        // Prevent adding duplicate podcasts based on URL (optional, but good practice)
        if self.podcasts.iter().any(|p| p.url() == podcast.url()) {
            println!("[APP] Podcast {} already exists. Skipping add.", podcast.title());
            // Optionally, you might want to update the existing one if the new one is fresher.
            // For now, we just skip.
            return;
        }

        let was_empty = self.podcasts.is_empty();
        self.podcasts.push(podcast);
        if was_empty {
            // Select the first podcast and its first episode
            // This will also call update_show_notes_content
            self.select_first_podcast();
        }
        // If not empty, the current selection is preserved.
    }

    // ============================ Handle default podcast selection ===============================
    // Select first Podcast as default, also Episode and Show Notes
    pub fn select_first_podcast(&mut self) {
        if !self.podcasts.is_empty() {
            self.selected_podcast_index = Some(0); // Select the first podcast

            // Optionally, also select the first episode of that podcast
            if let Some(first_podcast) = self.podcasts.first() {
                if !first_podcast.episodes().is_empty() {
                    self.selected_episode_index = Some(0);
                    self.episodes_list_ui_state.select(Some(0));
                } else {
                    self.selected_episode_index = None;
                    self.episodes_list_ui_state.select(None);
                }
            }
        } else {
            // No podcasts, so no selection
            self.selected_podcast_index = None;
            self.selected_episode_index = None;
            self.episodes_list_ui_state.select(None);
        }
        // When the list of podcasts changes or is initialized,
        // reset the episode list's scroll offset.
        *self.episodes_list_ui_state.offset_mut() = 0;
        self.focused_panel = FocusedPanel::Podcasts;
        self.update_show_notes_content();
    }

    // ===================================== Update show notes =====================================
    // This method is called when selection changes or app starts.
    // It's crucial for keeping show notes up-to-date.
    fn update_show_notes_content(&mut self) {
        let new_content = if let Some(episode) = self.selected_episode() {
            format_episode_description(episode.description())
        } else if self.selected_podcast().is_some() {
            "Select an episode to see its show notes.".to_string()
        } else {
            "Select a podcast and then an episode to see show notes.".to_string()
        };
        self.show_notes_state.set_content(new_content);
    }

    // =========================== Navigation methods for focused panel ============================
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

    // ========================== Scrolling within the focused panel list ==========================
    pub fn select_next_podcast(&mut self) {
        if self.podcasts.is_empty() {
            // Clear selection if empty
            self.selected_podcast_index = None;
            self.selected_episode_index = None;
            self.episodes_list_ui_state.select(None); // Reset ListState selection
            *self.episodes_list_ui_state.offset_mut() = 0; // Reset offset
            self.update_show_notes_content(); // Update show notes (will show placeholder)
            return;
        }

        let max_index: usize = self.podcasts.len() - 1;
        let new_idx: usize = match self.selected_podcast_index {
            Some(i) => {
                if i < max_index {
                    i + 1
                } else {
                    i
                }
            }
            None => 0, // If nothing selected, select the first
        };
        self.selected_podcast_index = Some(new_idx);
        self.selected_episode_index = None; // Reset episode selection for new podcast
        self.episodes_list_ui_state.select(None);
        *self.episodes_list_ui_state.offset_mut() = 0; // Reset offset for new episode list

        // Auto-select the first episode of the newly selected podcast
        if let Some(podcast) = self.selected_podcast() {
            if !podcast.episodes().is_empty() {
                self.selected_episode_index = Some(0);
                self.episodes_list_ui_state.select(Some(0));
            }
        }
        self.update_show_notes_content(); // Update content and reset scroll for new podcast/episode
    }

    pub fn select_prev_podcast(&mut self) {
        if self.podcasts.is_empty() {
            // Clear selection if empty
            self.selected_podcast_index = None;
            self.selected_episode_index = None;
            self.episodes_list_ui_state.select(None); // Reset ListState selection
            *self.episodes_list_ui_state.offset_mut() = 0; // Reset offset
            self.update_show_notes_content(); // Update show notes (will show placeholder)
            return;
        }
        let new_idx: usize = match self.selected_podcast_index {
            Some(i) => {
                if i > 0 {
                    i - 1
                } else {
                    i
                }
            }
            None => 0, // If nothing selected, select the first
        };

        self.selected_podcast_index = Some(new_idx);
        self.selected_episode_index = None;
        self.episodes_list_ui_state.select(None);
        *self.episodes_list_ui_state.offset_mut() = 0; // Reset offset for new episode list

        if let Some(podcast) = self.selected_podcast() {
            if !podcast.episodes().is_empty() {
                self.selected_episode_index = Some(0);
                self.episodes_list_ui_state.select(Some(0));
            }
        }
        self.update_show_notes_content();
    }

    // ==================================== Scrolling EPISODEs =====================================
    pub fn select_next_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            let episodes: &[Episode] = podcast.episodes();
            if episodes.is_empty() {
                self.selected_episode_index = None;
                self.episodes_list_ui_state.select(None);
                self.update_show_notes_content(); // Update to "no episodes" message
                return;
            }

            let max_index: usize = episodes.len() - 1;
            let new_idx: usize = match self.episodes_list_ui_state.selected() {
                Some(current_idx) => {
                    if current_idx < max_index {
                        current_idx + 1
                    } else {
                        current_idx
                    }
                }
                None => 0, // If nothing selected, select the first
            };

            self.selected_episode_index = Some(new_idx);
            self.episodes_list_ui_state.select(Some(new_idx));
            self.update_show_notes_content();
        } else {
            // No podcast selected, ensure episode index is None
            self.selected_episode_index = None;
            self.episodes_list_ui_state.select(None);
            self.update_show_notes_content();
        }
    }

    pub fn select_prev_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            let episodes: &[Episode] = podcast.episodes();
            if episodes.is_empty() {
                self.selected_episode_index = None;
                self.episodes_list_ui_state.select(None);
                self.update_show_notes_content();
                return;
            }

            let new_idx: usize = match self.episodes_list_ui_state.selected() {
                Some(current_idx) => {
                    if current_idx > 0 {
                        current_idx - 1
                    } else {
                        current_idx
                    }
                }
                None => 0, // If nothing selected, select the first
            };
            self.selected_episode_index = Some(new_idx);
            self.episodes_list_ui_state.select(Some(new_idx));
            self.update_show_notes_content();
        } else {
            // No podcast selected, clear episode selection
            self.selected_episode_index = None;
            self.episodes_list_ui_state.select(None);
            self.update_show_notes_content();
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
        self.add_podcast(test_podcast);
    }
}

// ==================================== App Startup and UI Loop ====================================

// This function will be responsible for loading podcasts from disk at startup.
// For now, it's a placeholder.
// The load_podcasts_from_disk function loads all podcasts at once. For large podcast libraries, consider:
//
//     Loading podcasts lazily
//     Adding pagination
//     Implementing a search/filter functionality
pub fn load_podcasts_from_disk() -> Vec<Podcast> {
    let mut loaded_podcasts = Vec::new();
    let data_dir = PathBuf::from(PODCAST_DATA_DIR); // Use the same constant

    // Load podcasts from disk, if any
    // TODO: Collect errors and display them in the TUI (e.g., a startup error message or a log panel).
    // TODO: Or, have load_podcasts_from_disk return a Result<Vec<Podcast>, LoadError> to propagate issues more formally.
    if data_dir.is_dir() {
        match fs::read_dir(data_dir) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                            if let Ok(json_content) = fs::read_to_string(&path) {
                                match serde_json::from_str::<Podcast>(&json_content) {
                                    Ok(podcast) => {
                                        println!("[APP Load] Loaded podcast: {}", podcast.title());
                                        loaded_podcasts.push(podcast);
                                    }
                                    Err(e) => eprintln!(
                                        "[APP Load] Failed to deserialize podcast from {:?}: {}",
                                        path, e
                                    ),
                                }
                            } else {
                                eprintln!("[APP Load] Failed to read file {:?}", path);
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("[APP Load] Failed to read podcast data directory: {}", e),
        }
    }
    // Sort podcasts by title, for example, for consistent ordering
    loaded_podcasts.sort_by(|a, b| a.title().cmp(b.title()));
    loaded_podcasts
}

pub fn start_ui(initial_app: Option<App>) -> Result<()> {
    // Set up the terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // If no app is provided (e.g., if start_ui was called from somewhere else without pre-configuration),
    // create a new, default/empty one.
    // main.rs is now expected to always pass Some(app) where 'app' is fully initialized.
    let mut app = initial_app.unwrap_or_else(|| {
        println!("[Warning] start_ui called with None; creating a default empty App instance.");
        let (_tx, event_rx) = broadcast::channel::<AppEvent>(32);
        App::new(event_rx)
    });

    run_app_loop(&mut terminal, &mut app)?;

    // Restore the terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}

pub fn run_app_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        // 1. Handle any pending application events (e.g., new podcast downloaded)
        app.handle_pending_events(); // This will call app.add_podcast if an event is received

        // 2. Prepare layout dependent state (like show notes scroll dimensions)
        let frame_size = terminal.get_frame().size(); // Fetch once before drawing
        crate::terminal_ui::prepare_ui_layout(app, frame_size);

        // 3. Draw the UI
        terminal.draw(|f| crate::terminal_ui::ui::<B>(f, app))?;

        // 4. Poll for input events with a timeout
        if event::poll(std::time::Duration::from_millis(100))? {
            // Poll with timeout
            if let Event::Key(key_event) = event::read()? {
                // key_event not just key
                if key_event.kind == event::KeyEventKind::Press {
                    // Process only key presses
                    app.on_key(key_event.code);
                }
            }
        }
    }

    Ok(())
}
