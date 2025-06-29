// src/player/mod.rs
mod command;
mod event;
pub mod player;

// Re-export for convenience
pub use command::PlayerRemoteCommand;
pub use event::PlayerEvent;
pub use player::AudioPlayer;
