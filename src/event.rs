// src/event.rs
use crate::podcast::{Podcast};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Emitted when a single podcast is fully processed (downloaded, parsed from an OPML entry)
    /// and is ready to be added to the application's main list.
    PodcastReadyForApp {
        podcast: Podcast,
        timestamp: DateTime<Utc>,
    },
}
