// src/podcast.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

// === PODCAST STRUCTURES ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodcastURL(String);

impl std::fmt::Display for PodcastURL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for PodcastURL {
    fn eq(&self, other: &Self) -> bool {
        // Normalize URLs by trimming trailing slashes
        let a = self.0.trim_end_matches('/');
        let b = other.0.trim_end_matches('/');
        a == b
    }
}

impl Eq for PodcastURL {}

impl PodcastURL {
    pub fn new(s: &str) -> Self {
        PodcastURL(s.to_string())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for PodcastURL {
    // Useful for passing to functions expecting &str
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// === EPISODE STRUCTURES ===
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeID(String);

impl std::fmt::Display for EpisodeID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl EpisodeID {
    pub fn new(s: &str) -> Self {
        EpisodeID(s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Podcast {
    #[serde(rename = "url")]
    url: PodcastURL,
    #[serde(rename = "title")]
    title: String,
    #[serde(rename = "description")]
    description: Option<String>,
    #[serde(rename = "image_url")]
    image_url: Option<String>,
    #[serde(rename = "website_url")]
    website_url: Option<String>,
    #[serde(rename = "episodes")]
    episodes: Vec<Episode>,
    #[serde(rename = "last_updated")]
    last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    #[serde(rename = "id")]
    id: EpisodeID,
    #[serde(rename = "title")]
    title: String,
    #[serde(rename = "description")]
    description: Option<String>,
    #[serde(rename = "published_date")]
    published_date: DateTime<Utc>,
    #[serde(rename = "duration")]
    duration: Option<String>,
    #[serde(rename = "audio_url")]
    audio_url: String,
    #[serde(rename = "size_in_bytes")]
    size_in_bytes: Option<u64>,
}

impl Podcast {
    pub fn new(
        url: PodcastURL,
        title: String,
        description: Option<String>,
        image_url: Option<String>,
        website_url: Option<String>,
        episodes: Vec<Episode>,
    ) -> Self {
        Self { url, title, description, image_url, website_url, episodes, last_updated: Utc::now() }
    }
    // Accessor methods

    pub fn url(&self) -> &PodcastURL {
        &self.url
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn image_url(&self) -> Option<&str> {
        self.image_url.as_deref()
    }

    pub fn website_url(&self) -> Option<&str> {
        self.website_url.as_deref()
    }

    pub fn episodes(&self) -> &[Episode] {
        &self.episodes
    }

    pub fn last_updated(&self) -> DateTime<Utc> {
        self.last_updated
    }

    // Mutable accessor for adding episodes
    pub fn add_episode(&mut self, episode: Episode) {
        self.episodes.push(episode);
    }
}

impl Episode {
    pub fn new(
        id: EpisodeID,
        title: String,
        description: Option<String>,
        published_date: DateTime<Utc>,
        duration: Option<String>,
        audio_url: String,
        size_in_bytes: Option<u64>,
    ) -> Self {
        Self { id, title, description, published_date, duration, audio_url, size_in_bytes }
    }

    pub fn id(&self) -> &EpisodeID {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn published_date(&self) -> DateTime<Utc> {
        self.published_date
    }

    pub fn duration(&self) -> Option<&str> {
        self.duration.as_deref()
    }

    pub fn audio_url(&self) -> &str {
        &self.audio_url
    }

    pub fn size_in_bytes(&self) -> Option<u64> {
        self.size_in_bytes
    }
}

impl fmt::Display for Podcast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Title       : {}", self.title)?;
        writeln!(f, "URL         : {}", self.url)?;
        if let Some(desc) = &self.description {
            writeln!(f, "Description : {}", desc)?;
        }
        if let Some(img) = &self.image_url {
            writeln!(f, "Image URL   : {}", img)?;
        }
        if let Some(web) = &self.website_url {
            writeln!(f, "Website URL : {}", web)?;
        }
        writeln!(f, "Episodes    : {}", self.episodes.len())?;
        writeln!(f, "Last updated: {}", self.last_updated)
    }
}
