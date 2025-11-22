// src/player/commands.rs
use crate::podcast::Episode;

#[derive(Debug, Clone)]
pub enum PlayerRemoteCommand {
    /// Plays a new episode. If another is playing, it stops it first.
    PlayEpisode { episode: Episode },
    /// Toggles pause/resume.
    TogglePlayPause,
    /// Stops playback and clears the current episode.
    Stop,
    /// Skips forward by a given number of seconds.
    SkipForward(u64),
    /// Skips backward by a given number of seconds.
    SkipBackward(u64),
    /// Seeks to a specific position in the track.
    SeekTo(std::time::Duration),
    /// Adjusts volume up.
    VolumeUp(f32),
    /// Adjusts volume down.
    VolumeDown(f32),
    /// Toggles mute.
    ToggleMute,
    /// Requests the player task to shut down.
    Quit,
}
