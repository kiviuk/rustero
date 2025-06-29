// src/event.rs
use crate::podcast::Podcast;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// During an OPML import, many sub-interpreters are spawned.
    /// Each one gets a clone of the event_tx and can independently report its success
    /// back to the main application.
    /// Emitted when a single podcast is fully processed (downloaded, parsed from an OPML entry)
    /// and is ready to be added to the application's main list.
    PodcastReadyForApp { podcast: Podcast, timestamp: DateTime<Utc> },
}
