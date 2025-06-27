// src/player/mod.rs
pub mod player;
mod event;
mod command;

// Re-export for convenience
pub use command::PlayerCommand;
pub use event::PlayerEvent;
pub use player::AudioPlayer;