use chrono::Utc;
use rustero::app::{self, App};
use rustero::commands::command_interpreters::PodcastPipelineInterpreter;
use rustero::commands::podcast_algebra::{CommandAccumulator, PipelineData, run_commands};
use rustero::commands::podcast_commands::PodcastCmd;
use rustero::podcast::{Episode, EpisodeID, Podcast, PodcastURL};
use rustero::podcast_download::{FeedFetcher, HttpFeedFetcher};
use std::sync::Arc;

const SAMPLE_SHOW_NOTES_1: &str = r#"
<h1>Welcome to Episode 42: The Future of Rust</h1>

<p>In this episode, we talk with <strong>Jane Doe</strong>, a senior engineer at Rustaceans Inc., about what's coming next in the Rust ecosystem.</p>

<ul>
  <li>🦀 The impact of <a href="https://blog.rust-lang.org">Rust 2024 Edition</a></li>
  <li>📦 Tips for managing large crates</li>
  <li>🛠️ Async/Await best practices in real-world projects</li>
</ul>

<blockquote>
  “Rust lets us write safe, fast code — and helps us sleep better at night.” – Jane Doe
</blockquote>

<h2>Resources Mentioned</h2>
<ol>
  <li><a href="https://doc.rust-lang.org/book/">The Rust Book</a></li>
  <li><a href="https://tokio.rs">Tokio Async Runtime</a></li>
  <li><a href="https://serde.rs">Serde Serialization</a></li>
</ol>

<p>Be sure to <strong>subscribe</strong> and leave us a review on your favorite podcast app!</p>
"#;
const SAMPLE_SHOW_NOTES_2: &str = r#"
123456789012345678901234567890
223456789012345678901234567890
323456789012345678901234567890
423456789012345678901234567890
523456789012345678901234567890
623456789012345678901234567890
723456789012345678901234567890
823456789012345678901234567890
923456789012345678901234567890
102345678901234567890123456789
112345678901234567890123456789
122345678901234567890123456789
13v
142345678901234567890123456789
152345678901234567890123456789
16v
172345678901234567890123456789
182345678901234567890123456789
192345678901234567890123456789
202345678901234567890123456789
212345678901234567890123456789
222345678901234567890123456789
232345678901234567890123456789
"#;

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

    println!("{}", result1.is_err());

    // Create test episodes using the proper constructor
    let test_episodes_1 = vec![
        Episode::new(
            EpisodeID::new("ep1"),
            "First Episode".to_string(),
            Some(SAMPLE_SHOW_NOTES_2.to_string()),
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
    app.add_podcast(test_podcast);

    // Add another test podcast
    let test_podcast2 = Podcast::new(
        PodcastURL::new("http://example.com/feed2"),
        "Programming Tips".to_string(),
        Some("Programming tips and tricks".to_string()),
        None,
        None,
        test_episodes_2.clone(),
    );
    app.add_podcast(test_podcast2);

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
