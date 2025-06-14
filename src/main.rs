// src/main.rs
use chrono::Utc;
use clap::Parser;
// Import Parser
use rustero::app::{self, App, load_podcasts_from_disk};
use rustero::event::AppEvent;
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
// Logging
use anyhow::anyhow;
use log::{LevelFilter, debug, error, info, warn}; // Import log macros // For error handling

// This function now *always* configures file logging
fn setup_logger() -> anyhow::Result<()> {
    // Removed is_headless parameter
    let log_file_path = "castero.log";
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {} - {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(LevelFilter::Info) // Default level for all modes
        // Suppress verbose logs from external crates that you don't control
        .level_for("reqwest", LevelFilter::Warn)
        .level_for("hyper", LevelFilter::Warn)
        .level_for("h2", LevelFilter::Warn) // Often verbose with HTTP/2
        // Output to a file
        .chain(fern::log_file(log_file_path)?)
        // Optionally, also output errors to stderr (can be useful even in TUI, but might still conflict)
        // .chain(
        //     fern::Dispatch::new()
        //         .level(LevelFilter::Error) // Only errors go to stderr
        //         .chain(std::io::stderr())
        // )
        .apply()?;

    info!("Logging all output to file: {}", log_file_path); // This message will go to the file.
    // On initial run, you might see it briefly before TUI takes over.
    Ok(())
}
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

    setup_logger()?;

    // Channel for events
    let (event_tx_main, app_event_rx): (Sender<AppEvent>, Receiver<AppEvent>) =
        broadcast::channel::<AppEvent>(32);

    // --- CLI Command Processing (if --import-opml-file is present) ---
    if let Some(opml_path) = args.import_opml_file {
        info!("--- Processing OPML import from: {} ---", opml_path.display());

        // Construct the command chain: Load file -> Process entries -> End
        let cmd_import_opml: PodcastCmd = PodcastCmd::load_opml_file(
            opml_path,
            PodcastCmd::process_opml_entries(
                vec![], // This vec is now a placeholder; content comes from accumulator
                PodcastCmd::end(),
            ),
        );

        let fetcher: Arc<HttpFeedFetcher> = Arc::new(HttpFeedFetcher::new());
        let mut interpreter: PodcastPipelineInterpreter =
            PodcastPipelineInterpreter::new(fetcher.clone(), event_tx_main.clone());

        let initial_acc: CommandAccumulator = Ok(PipelineData::default());
        let import_result: CommandAccumulator =
            run_commands(&cmd_import_opml, initial_acc, &mut interpreter).await;

        match import_result {
            Ok(_) => info!("OPML import completed successfully."),
            Err(e) => {
                error!("Error: OPML import failed. Check log for details: {}", e);
                return Err(anyhow!(e));
            }
        }

        if args.headless {
            // Exit if in headless mode after import
            info!("Headless import finished. Exiting.");
            return Ok(());
        }
    }

    // =================================== TUI APPLICATION START ====================================
    let mut app: App = App::new(app_event_rx);

    // 1. Load podcasts from disk first
    let disk_podcasts: Vec<Podcast> = load_podcasts_from_disk(); // This function needs to be public in app.rs
    if !disk_podcasts.is_empty() {
        // Add loaded podcasts to the app.
        // The add_podcast method handles duplicates and selecting the first if the app was empty.
        for podcast in disk_podcasts {
            app.add_podcast(podcast);
        }
    }

    app::start_ui(Some(app))
}
