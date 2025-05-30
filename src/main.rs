use anyhow::{Result, anyhow};
use chrono::Utc;
use rustero::app::{self, App};
use rustero::commands::command_interpreters::PodcastPipelineInterpreter;
use rustero::commands::podcast_algebra::{CommandAccumulator, PipelineData, run_commands};
use rustero::commands::podcast_commands::PodcastCmd;
use rustero::podcast::{Episode, EpisodeID, Podcast, PodcastURL};
use rustero::podcast_download::{FeedFetcher, HttpFeedFetcher};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create new app instance
    let mut app = App::new();

    let fetcher: Arc<dyn FeedFetcher + Send + Sync> = Arc::new(HttpFeedFetcher::new());
    let mut interpreter = PodcastPipelineInterpreter::new(fetcher.clone());

    let cmd_seq1 = PodcastCmd::eval_url_from_str(
        "https://feeds.zencastr.com/f/oSn1i316.rss", // URL as string for EvalUrl
        PodcastCmd::download(
            // This URL is a fallback if EvalUrl somehow didn't populate the accumulator
            // or if the interpreter logic for Download was different.
            // With current interpreter, eval'd URL takes precedence.
            PodcastURL::new("http://unused-fallback.com/rss"),
            PodcastCmd::save(PodcastCmd::end()),
        ),
    );

    println!("--- Running Sequence 1: Eval -> Download -> Save ---");
    let initial_acc: CommandAccumulator = Ok(PipelineData::default());
    let result1 = run_commands(&cmd_seq1, initial_acc, &mut interpreter).await;

    // Create test episodes using the proper constructor
    let test_episodes_1 = vec![
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

    let test_episodes_2 = vec![
        Episode::new(
            EpisodeID::new("ep10"),
            "10th Episode".to_string(),
            Some("This is episode 10".to_string()),
            Utc::now(),
            Some("20:00".to_string()),
            "http://example.com/ep10.mp3".to_string(),
            Some(1024 * 1024), // 1MB size
        ),
        Episode::new(
            EpisodeID::new("ep11"),
            "11th Episode".to_string(),
            Some("This is episode 11".to_string()),
            Utc::now(),
            Some("25:00".to_string()),
            "http://example.com/ep11.mp3".to_string(),
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
        test_episodes_1.clone(),
    );
    app.podcasts.push(test_podcast);

    // Add another test podcast
    let test_podcast2 = Podcast::new(
        PodcastURL::new("http://example.com/feed2"),
        "Programming Tips".to_string(),
        Some("Programming tips and tricks".to_string()),
        None,
        None,
        test_episodes_2.clone(),
    );
    app.podcasts.push(test_podcast2);



    // match result1 {
    //     Ok(data) => {
    //         println!("\nSequence 1 completed successfully.");
    //         if let Some(p) = data.current_podcast {
    //             // current_podcast should still be Some after save
    //             println!("Last processed podcast in accumulator: {}", p);
    //         } else {
    //             println!(
    //                 "Sequence 1 completed, but no podcast was in the final accumulator context."
    //             );
    //         }
    //
    //         Ok(()) // Explicitly return Ok(()) for the success case of main
    //     }
    //     Err(pipeline_err) => {
    //         eprintln!("\nSequence 1 failed: {}", pipeline_err);
    //         Err(anyhow!(pipeline_err)) // Using anyhow! macro
    //     }
    // }
    //
    // Start the UI with our initialized app
    app::start_ui(Some(app))


}
