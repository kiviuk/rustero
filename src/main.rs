use anyhow::Result;
use rustero::podcast::{Podcast, Episode, EpisodeID, PodcastURL};
use chrono::Utc;
use rustero::app::{self, App};

fn main() -> Result<()> {
    // Create new app instance
    let mut app = App::new();

    // Create test episodes using the proper constructor
    let test_episodes = vec![
        Episode::new(
            EpisodeID::new("ep1"),
            "First Episode".to_string(),
            Some("This is episode 1".to_string()),
            Utc::now(),
            Some("20:00".to_string()),
            "http://example.com/ep1.mp3".to_string(),
            Some(1024 * 1024), // 1MB size
        ),
        Episode::new(
            EpisodeID::new("ep2"),
            "Second Episode".to_string(),
            Some("This is episode 2".to_string()),
            Utc::now(),
            Some("25:00".to_string()),
            "http://example.com/ep2.mp3".to_string(),
            Some(1024 * 1024 * 2), // 2MB size
        ),
    ];

    // Create a test podcast with episodes
    let test_podcast = Podcast::new(
        PodcastURL::new("http://example.com/feed1"),
        "Rust Daily News".to_string(),
        Some("Daily news about Rust".to_string()),
        None,
        None,
        test_episodes,
    );
    app.podcasts.push(test_podcast);

    // Add another test podcast
    let test_podcast2 = Podcast::new(
        PodcastURL::new("http://example.com/feed2"),
        "Programming Tips".to_string(),
        Some("Programming tips and tricks".to_string()),
        None,
        None,
        vec![],
    );
    app.podcasts.push(test_podcast2);

    // Start the UI with our initialized app
    app::start_ui(Some(app))
}

