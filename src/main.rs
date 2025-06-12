// src/main.rs
use chrono::Utc;
use clap::Parser;
// Import Parser
use rustero::app::{self, App, load_podcasts_from_disk};
use rustero::event::AppEvent;
use rustero::opml::opml_parser::OpmlFeedEntry;
use rustero::podcast::{Episode, EpisodeID, Podcast, PodcastURL};
use std::path::PathBuf;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
// For file paths
use rustero::commands::podcast_algebra::{CommandAccumulator, PipelineData, run_commands};
use rustero::commands::podcast_commands::PodcastCmd;
use rustero::commands::podcast_pipeline_interpreter::PodcastPipelineInterpreter;
use std::sync::Arc;
// For Arc<FeedFetcher>
use rustero::podcast_download::HttpFeedFetcher;

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

#[derive(Parser, Debug)]
#[command(author, version, about = "A TUI podcast client.", long_about = None)]
struct Args {
    /// Path to an OPML file to import podcasts from.
    #[arg(long, value_name = "FILE")]
    import_opml_file: Option<PathBuf>,

    /// Run in headless mode (no TUI) for operations like import.
    #[arg(long)]
    headless: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = Args::parse();

    // Channel for events
    let (event_tx_main, app_event_rx): (Sender<AppEvent>, Receiver<AppEvent>) =
        broadcast::channel::<AppEvent>(32);

    let fetcher: Arc<HttpFeedFetcher> = Arc::new(HttpFeedFetcher::new());
    let mut interpreter: PodcastPipelineInterpreter =
        PodcastPipelineInterpreter::new(fetcher.clone(), event_tx_main.clone());

    // --- CLI Command Processing ---
    if let Some(opml_path) = args.import_opml_file {
        println!("--- Processing OPML import from: {} ---", opml_path.display());

        // Construct the command chain: Load file -> Process entries -> End
        let cmd_import_opml: PodcastCmd = PodcastCmd::load_opml_file(
            opml_path,
            PodcastCmd::process_opml_entries(
                vec![], // This vec is now a placeholder; content comes from accumulator
                PodcastCmd::end(),
            ),
        );

        let initial_acc: CommandAccumulator = Ok(PipelineData::default());
        let import_result: CommandAccumulator =
            run_commands(&cmd_import_opml, initial_acc, &mut interpreter).await;

        match import_result {
            Ok(_) => println!("OPML import completed successfully."),
            Err(e) => {
                eprintln!("OPML import failed: {}", e);
                return Err(anyhow::anyhow!(e)); // Exit with error for CLI
            }
        }

        if args.headless {
            // Exit if in headless mode after import
            println!("Headless import finished. Exiting.");
            return Ok(());
        }
    }

    // If not in headless mode (or if headless import didn't happen/failed), proceed to TUI
    // Create a new app instance. It will load saved podcasts from disk internally.
    let mut app = App::new(app_event_rx);

    // 1. Load podcasts from disk first
    let disk_podcasts: Vec<Podcast> = load_podcasts_from_disk(); // This function needs to be public in app.rs
    if !disk_podcasts.is_empty() {
        // Add loaded podcasts to the app.
        // The add_podcast method handles duplicates and selecting the first if the app was empty.
        for podcast in disk_podcasts {
            app.add_podcast(podcast);
        }
    }

    // let fetcher: Arc<dyn FeedFetcher + Send + Sync> = Arc::new(HttpFeedFetcher::new());
    // let mut interpreter = PodcastPipelineInterpreter::new(fetcher.clone(), event_tx.clone());
    //
    // let cmd_seq1 = PodcastCmd::eval_url_from_str(
    //     "https://feeds.zencastr.com/f/oSn1i316.rss", // URL as string for EvalUrl
    //     PodcastCmd::download(
    //         // This URL is a fallback if EvalUrl somehow didn't populate the accumulator
    //         // or if the interpreter logic for Download was different.
    //         // With current interpreter, eval'd URL takes precedence.
    //         PodcastURL::new("http://unused-fallback.com/rss"),
    //         PodcastCmd::save(PodcastCmd::end()),
    //     ),
    // );
    //
    // println!("--- Running Sequence 1: Eval -> Download -> Save ---");
    // let initial_acc: CommandAccumulator = Ok(PipelineData::default());
    // let result1 = run_commands(&cmd_seq1, initial_acc, &mut interpreter).await;
    //
    // println!("{}", result1.is_err());

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

    let dummy_opml_entries: Vec<OpmlFeedEntry> = vec![
        OpmlFeedEntry {
            title: "Developer Voices".to_string(),
            xml_url: "https://feeds.zencastr.com/f/oSn1i316.rss".to_string(),
            html_url: None,
        },
        OpmlFeedEntry {
            title: "GOTO - The Brightest Minds in Tech".to_string(),
            xml_url: "https://feeds.buzzsprout.com/1714721.rss".to_string(),
            html_url: None,
        },
        // Add a known failing one if you want to test error path (optional for this step)
        // OpmlFeedEntry {
        //     title: "Test Feed 3 (NonExistent)".to_string(),
        //     xml_url: "http://nonexistentfeed.example.com/rss".to_string(),
        //     html_url: None,
        // },
    ];

    // let cmd_process_opml: PodcastCmd =
    //     PodcastCmd::process_opml_entries(dummy_opml_entries, PodcastCmd::end());
    // let fetcher: Arc<dyn FeedFetcher + Send + Sync> = Arc::new(HttpFeedFetcher::new());
    //
    // let mut interpreter: PodcastPipelineInterpreter =
    //     PodcastPipelineInterpreter::new(fetcher.clone(), event_tx.clone());
    // let initial_acc_for_opml: CommandAccumulator = Ok(PipelineData::default());
    //
    // println!("--- Running OPML Entry Processing Sequence ---");
    // let opml_processing_result: CommandAccumulator =
    //     run_commands(&cmd_process_opml, initial_acc_for_opml, &mut interpreter).await;
    //
    // match opml_processing_result {
    //     Ok(_) => println!("OPML entry processing command finished."),
    //     Err(e) => eprintln!("OPML entry processing command failed: {:?}", e),
    // }

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
