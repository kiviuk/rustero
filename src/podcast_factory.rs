use crate::errors::DownloaderError;
use crate::podcast::{Podcast, PodcastURL};
use anyhow::Result;
use rss::Channel;

#[derive(Debug)]
pub struct ParsedFeed {
    pub channel: Channel,
}

#[derive(Debug, Clone, Copy)]
pub enum EpisodeSortOrder {
    NewestFirst,
    OldestFirst,
}

pub struct PodcastFactory {
    episode_limit: Option<usize>,
    sort_order: EpisodeSortOrder,
}

impl Default for PodcastFactory {
    fn default() -> Self {
        Self { episode_limit: None, sort_order: EpisodeSortOrder::NewestFirst }
    }
}

impl PodcastFactory {
    pub fn new() -> Self {
        Self::default()
    }

    // Builder methods
    pub fn with_episode_limit(mut self, limit: usize) -> Self {
        self.episode_limit = Some(limit);
        self
    }

    pub fn with_sort_order(mut self, order: EpisodeSortOrder) -> Self {
        self.sort_order = order;
        self
    }

    pub fn create_podcast(
        &self,
        parsed: ParsedFeed,
        feed_url: String,
    ) -> Result<Podcast, DownloaderError> {
        Ok(Podcast::new(
            PodcastURL::new(&feed_url),
            parsed.channel.title().to_string(),
            Some(parsed.channel.description()).map(String::from),
            parsed.channel.image().map(|img| img.url().to_string()),
            Some(parsed.channel.link().to_string()),
            vec![], // Empty episodes for now, will be populated based on configuration
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rss::{ChannelBuilder, ImageBuilder};

    #[test]
    fn test_create_podcast_from_parsed_feed() {
        // Create a minimal RSS Channel for testing
        let factory = PodcastFactory::new()
            .with_episode_limit(10)
            .with_sort_order(EpisodeSortOrder::NewestFirst);

        let image = ImageBuilder::default().url("http://example.com/image.jpg".to_string()).build();

        let url = "http://example.com/feed".to_string();
        let channel = ChannelBuilder::default()
            .title("Test Podcast".to_string())
            .link(url.to_string())
            .description("Test Description".to_string())
            .image(image)
            .build();

        let parsed = ParsedFeed { channel };
        let podcast = factory.create_podcast(parsed, url).unwrap();

        // Verify the basic fields are correctly mapped
        assert_eq!(podcast.title(), "Test Podcast");
        assert_eq!(podcast.url(), &PodcastURL::new("http://example.com/feed"));
        assert_eq!(podcast.description(), Some("Test Description"));
        assert_eq!(podcast.image_url(), Some("http://example.com/image.jpg"));
        assert_eq!(podcast.website_url(), Some("http://example.com/feed"));
        assert!(podcast.episodes().is_empty());
    }
}
