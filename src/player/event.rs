// src/player/events.rs
use std::time::Duration;
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    /// Emitted when an episode starts playing.
    Playing { podcast_title: String, episode_title: String, duration: Duration },
    /// Emitted when playback is paused.
    Paused,
    /// Emitted when playback is resumed.
    Resumed,
    /// Emitted when playback is stopped (e.g., by user or finished episode).
    Stopped,
    /// Emitted when the player is buffering content.
    Buffering,
    /// Emitted periodically to update playback progress.
    Progress { current_position: Duration, total_duration: Duration },
    /// Emitted when the volume changes.
    VolumeChanged(f32), // Current volume level (0.0 to 1.0+)
    /// Emitted when an error occurs during playback.
    Error(String),
    /// Emitted when the current episode finishes.
    EpisodeEnded,
}
