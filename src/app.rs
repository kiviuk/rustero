// src/app.rs
use crate::commands::podcast_pipeline_interpreter::PODCAST_DATA_DIR;
use crate::event::PipelineEvent;
pub use crate::player::player::PlaybackStatus;
use crate::player::{PlayerEvent, PlayerRemoteCommand};
use crate::podcast::{Episode, Podcast};
use crate::terminal_ui::format_episode_description;
use crate::widgets::scrollable_paragraph::ScrollableParagraphState;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use log::{error, info, trace, warn};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::io::Stdout;
use std::path::PathBuf;
use std::time::Duration;
use std::{fs, io};
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FocusedPanel {
    Podcasts,
    Episodes,
    ShowNotes,
}

impl Default for FocusedPanel {
    fn default() -> Self {
        FocusedPanel::Podcasts
    }
}

pub struct App {
    pub should_quit: bool,
    pub podcasts: Vec<Podcast>,
    pub podcasts_list_ui_state: ListState,
    pub episodes_list_ui_state: ListState,
    pub focused_panel: FocusedPanel,
    pub show_notes_state: ScrollableParagraphState,
    pub event_rx: broadcast::Receiver<PipelineEvent>,
    pub player_command_tx: mpsc::Sender<PlayerRemoteCommand>,
    pub player_event_rx: broadcast::Receiver<PlayerEvent>,
    pub player_status: PlaybackStatus,
    pub current_playback_progress: Duration,
    pub total_playback_duration: Option<Duration>,
    pub current_player_episode: Option<(String, String)>,
}

impl App {
    pub fn new(
        app_event_rx: broadcast::Receiver<PipelineEvent>,
        player_command_tx: mpsc::Sender<PlayerRemoteCommand>,
        player_event_rx: broadcast::Receiver<PlayerEvent>,
    ) -> App {
        let mut app = App {
            should_quit: false,
            podcasts: Vec::new(),
            podcasts_list_ui_state: ListState::default(),
            episodes_list_ui_state: ListState::default(),
            focused_panel: FocusedPanel::default(),
            show_notes_state: ScrollableParagraphState::default(),
            event_rx: app_event_rx,
            player_command_tx,
            player_event_rx,
            player_status: PlaybackStatus::Stopped,
            current_playback_progress: Duration::default(),
            total_playback_duration: None,
            current_player_episode: None,
        };
        app.select_first_podcast();
        app
    }

    pub fn handle_pending_events(&mut self) {
        // Non-blocking try_recv is safe in a sync loop.
        match self.event_rx.try_recv() {
            Ok(PipelineEvent::PodcastReadyForApp { podcast, .. }) => {
                trace!("[APP] Received PodcastReadyForApp for: {}", podcast.title());
                self.add_podcast(podcast);
            }
            Err(broadcast::error::TryRecvError::Empty) => {}
            Err(e) => warn!("[APP] Event receiver error: {:?}", e),
        }

        match self.player_event_rx.try_recv() {
            Ok(event) => self.handle_player_event(event),
            Err(broadcast::error::TryRecvError::Empty) => {}
            Err(e) => warn!("[APP] Player event receiver error: {:?}", e),
        }
    }

    fn handle_player_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::Playing { podcast_title, episode_title, duration } => {
                self.player_status = PlaybackStatus::Playing;
                self.current_player_episode = Some((podcast_title, episode_title));
                self.total_playback_duration = Some(duration);
            }
            PlayerEvent::Paused => self.player_status = PlaybackStatus::Paused,
            PlayerEvent::Resumed => self.player_status = PlaybackStatus::Playing,
            PlayerEvent::Stopped | PlayerEvent::EpisodeEnded => {
                self.player_status = PlaybackStatus::Stopped;
                self.current_player_episode = None;
                self.current_playback_progress = Duration::default();
                self.total_playback_duration = None;
            }
            PlayerEvent::Buffering => self.player_status = PlaybackStatus::Buffering,
            PlayerEvent::Progress { current_position, total_duration } => {
                self.current_playback_progress = current_position;
                self.total_playback_duration = Some(total_duration);
            }
            PlayerEvent::VolumeChanged(vol) => info!("Volume changed to {}", vol),
            PlayerEvent::Error(msg) => {
                self.player_status = PlaybackStatus::Error;
                error!("[APP] Player Error: {}", msg);
            }
        }
    }

    pub fn add_podcast(&mut self, podcast: Podcast) {
        if self.podcasts.iter().any(|p| p.url() == podcast.url()) {
            info!("[APP] Podcast {} already exists. Skipping.", podcast.title());
            return;
        }
        let was_empty: bool = self.podcasts.is_empty();
        self.podcasts.push(podcast);
        if was_empty {
            self.select_first_podcast();
        }
    }

    pub fn select_first_podcast(&mut self) {
        if !self.podcasts.is_empty() {
            self.episodes_list_ui_state.select(Some(0));
            if let Some(first_podcast) = self.podcasts.first() {
                if !first_podcast.episodes().is_empty() {
                    self.podcasts_list_ui_state.select(Some(0));
                    self.episodes_list_ui_state.select(Some(0));
                } else {
                    self.podcasts_list_ui_state.select(None);
                    self.episodes_list_ui_state.select(None);
                }
            }
        } else {
            self.podcasts_list_ui_state.select(None);
            self.episodes_list_ui_state.select(None);
        }
        *self.podcasts_list_ui_state.offset_mut() = 0;
        *self.episodes_list_ui_state.offset_mut() = 0;
        self.update_show_notes_content();
    }

    fn update_show_notes_content(&mut self) {
        let new_content: String = if let Some(episode) = self.selected_episode() {
            format_episode_description(episode.description())
        } else if self.selected_podcast().is_some() {
            "Select an episode to see its show notes.".to_string()
        } else {
            "Select a podcast and then an episode to see show notes.".to_string()
        };
        self.show_notes_state.set_content(new_content);
    }

    pub fn focus_next_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Podcasts => FocusedPanel::Episodes,
            FocusedPanel::Episodes => FocusedPanel::ShowNotes,
            FocusedPanel::ShowNotes => FocusedPanel::Podcasts,
        };
    }

    pub fn focus_prev_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Podcasts => FocusedPanel::ShowNotes,
            FocusedPanel::Episodes => FocusedPanel::Podcasts,
            FocusedPanel::ShowNotes => FocusedPanel::Episodes,
        };
    }

    pub fn select_next_podcast(&mut self) {
        if self.podcasts.is_empty() {
            return;
        }
        let max_index: usize = self.podcasts.len() - 1;
        let current_podcast_index: Option<usize> = self.podcasts_list_ui_state.selected();
        let new_idx: Option<usize> = current_podcast_index.map(|idx| (idx + 1).min(max_index));
        self.podcasts_list_ui_state.select(new_idx);
        self.episodes_list_ui_state.select(Some(0));
        *self.episodes_list_ui_state.offset_mut() = 0;
        self.update_show_notes_content();
    }

    pub fn select_prev_podcast(&mut self) {
        if self.podcasts.is_empty() {
            return;
        }
        let current_podcast_index: Option<usize> = self.podcasts_list_ui_state.selected();
        let new_idx: Option<usize> = current_podcast_index.map(|i| i.saturating_sub(1));
        self.podcasts_list_ui_state.select(new_idx);
        self.episodes_list_ui_state.select(Some(0));
        *self.episodes_list_ui_state.offset_mut() = 0;
        self.update_show_notes_content();
    }

    pub fn select_next_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            if podcast.episodes().is_empty() {
                return;
            }
            let max_index: usize = podcast.episodes().len() - 1;
            let current_index: usize = self.episodes_list_ui_state.selected().unwrap_or(0);
            let new_index: usize = (current_index + 1).min(max_index);
            self.episodes_list_ui_state.select(Some(new_index));
            self.update_show_notes_content();
        }
    }

    pub fn select_prev_episode(&mut self) {
        if let Some(podcast) = self.selected_podcast() {
            if podcast.episodes().is_empty() {
                return;
            }
            let current_index: usize = self.episodes_list_ui_state.selected().unwrap_or(0);
            let new_index: usize = current_index.saturating_sub(1);
            self.episodes_list_ui_state.select(Some(new_index));
            self.update_show_notes_content();
        }
    }

    // This is now synchronous
    pub fn on_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char(' ') => {
                self.toggle_play_pause_action();
                return;
            }
            KeyCode::Enter => {
                if self.focused_panel == FocusedPanel::Episodes {
                    self.play_selected_episode_action();
                }
                return;
            }
            KeyCode::Char('-') => {
                self.send_player_command(PlayerRemoteCommand::VolumeDown(0.05));
                return;
            }
            KeyCode::Char('=') => {
                self.send_player_command(PlayerRemoteCommand::VolumeUp(0.05));
                return;
            }
            KeyCode::Char('m') => {
                 self.send_player_command(PlayerRemoteCommand::ToggleMute);
                 return;
            }
            _ => {}
        }

        match self.focused_panel {
            FocusedPanel::Podcasts => match key {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_podcast(),
                KeyCode::Up | KeyCode::Char('k') => self.select_prev_podcast(),
                KeyCode::Tab | KeyCode::Right => self.focus_next_panel(),
                KeyCode::BackTab | KeyCode::Left => self.focus_prev_panel(),
                _ => {}
            },
            FocusedPanel::Episodes => match key {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_episode(),
                KeyCode::Up | KeyCode::Char('k') => self.select_prev_episode(),
                KeyCode::Tab | KeyCode::Right => self.focus_next_panel(),
                KeyCode::BackTab | KeyCode::Left => self.focus_prev_panel(),
                _ => {}
            },
            FocusedPanel::ShowNotes => match key {
                KeyCode::Down | KeyCode::Char('j') => self.show_notes_state.scroll_down(1),
                KeyCode::Up | KeyCode::Char('k') => self.show_notes_state.scroll_up(1),
                KeyCode::PageDown => self.show_notes_state.scroll_down(10),
                KeyCode::PageUp => self.show_notes_state.scroll_up(10),
                KeyCode::Tab | KeyCode::Right => self.focus_next_panel(),
                KeyCode::BackTab | KeyCode::Left => self.focus_prev_panel(),
                _ => {}
            },
        }
    }

    // Use non-blocking try_send
    fn send_player_command(&mut self, command: PlayerRemoteCommand) {
        if self.player_command_tx.try_send(command).is_err() {
            error!("Failed to send player command: channel is full or closed.");
            self.player_status = PlaybackStatus::Error;
        }
    }

    fn play_selected_episode_action(&mut self) {
        if let Some(episode) = self.selected_episode() {
            info!(
                "Sending Play command for: '{}' with URL: {}",
                episode.title(),
                episode.audio_url()
            );
            self.send_player_command(PlayerRemoteCommand::PlayEpisode { episode: episode.clone() });
        } else {
            warn!("Play action triggered, but no episode is selected.");
        }
    }

    fn toggle_play_pause_action(&mut self) {
        info!("Sending TogglePlayPause command");
        self.send_player_command(PlayerRemoteCommand::TogglePlayPause);
    }

    pub fn selected_podcast(&self) -> Option<&Podcast> {
        self.podcasts_list_ui_state.selected().and_then(|i| self.podcasts.get(i))
    }

    pub fn selected_episode(&self) -> Option<&Episode> {
        self.episodes_list_ui_state.selected().and_then(|selected_index| {
            self.selected_podcast().and_then(|podcast| podcast.episodes().get(selected_index))
        })
    }
}

pub fn load_podcasts_from_disk() -> Vec<Podcast> {
    let mut loaded_podcasts: Vec<Podcast> = Vec::new();
    if let Ok(entries) = fs::read_dir(PODCAST_DATA_DIR) {
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(json_content) = fs::read_to_string(&path) {
                    match serde_json::from_str::<Podcast>(&json_content) {
                        Ok(podcast) => loaded_podcasts.push(podcast),
                        Err(e) => error!("Failed to deserialize {:?}: {}", path, e),
                    }
                }
            }
        }
    }
    loaded_podcasts.sort_by(|a, b| a.title().cmp(b.title()));
    loaded_podcasts
}

pub fn start_ui(mut app: App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout: Stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, DisableMouseCapture)?;
    let backend: CrosstermBackend<Stdout> = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    // blocks the thread it's running on (with crossterm::event::poll).
    run_app_loop(&mut terminal, &mut app)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// This is now synchronous
pub fn run_app_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        app.handle_pending_events();
        let frame_size: Rect = terminal.get_frame().size();
        crate::terminal_ui::prepare_ui_layout(app, frame_size);
        terminal.draw(|f| crate::terminal_ui::ui::<B>(f, app))?;

        // This blocking poll is now safe because we are on our own dedicated thread.
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    app.on_key(key_event.code);
                }
            }
        }
    }
    Ok(())
}
