// src/main.rs
use anyhow::anyhow;
use clap::Parser;
use log::{LevelFilter, error, info};
use rustero::app::{self, App, load_podcasts_from_disk};
use rustero::commands::podcast_algebra::{CommandAccumulator, PipelineData, run_commands};
use rustero::commands::podcast_commands::PodcastCmd;
use rustero::commands::podcast_pipeline_interpreter::PodcastPipelineInterpreter;
use rustero::event::PipelineEvent;
use rustero::player::{AudioPlayer, PlayerEvent, PlayerRemoteCommand};
use rustero::podcast::Podcast;
use rustero::podcast_download::HttpFeedFetcher;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;

fn setup_logger() -> anyhow::Result<()> {
    let log_file_path: &str = "rustero.log";
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
        .level(LevelFilter::Info)
        .level_for("reqwest", LevelFilter::Warn)
        .level_for("hyper", LevelFilter::Warn)
        .chain(fern::log_file(log_file_path)?)
        .apply()?;
    info!("Logging all output to file: {}", log_file_path);
    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about = "A TUI podcast client.", long_about = None)]
struct Args {
    // By default, clap converts the Rust field name from snake_case to kebab-case:
    // import_opml_file → import-opml-file
    // cargo run --release -- \
    //     --import-opml-file path/to/podcasts.opml \
    //     --headless
    // cargo run -- -h
    #[arg(long, value_name = "FILE")]
    import_opml_file: Option<PathBuf>,
    #[arg(long)]
    headless: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = Args::parse();
    setup_logger()?;

    // creates a multi-producer, multi-consumer communication channel.
    let (podcast_event_publisher, podcast_event_subscriber): (
        Sender<PipelineEvent>,
        Receiver<PipelineEvent>,
    ) = broadcast::channel::<PipelineEvent>(32);

    let (player_cmd_publisher, player_cmd_queue): (
        tokio::sync::mpsc::Sender<PlayerRemoteCommand>,
        tokio::sync::mpsc::Receiver<PlayerRemoteCommand>,
    ) = mpsc::channel::<PlayerRemoteCommand>(100);
    let (player_event_publisher, player_event_subscriber): (
        Sender<PlayerEvent>,
        Receiver<PlayerEvent>,
    ) = broadcast::channel::<PlayerEvent>(100);

    let mut player: AudioPlayer = AudioPlayer::new(player_cmd_queue, player_event_publisher)
        .expect("Failed to create AudioPlayer");

    if let Some(opml_path) = args.import_opml_file {
        info!("--- Processing OPML import from: {} ---", opml_path.display());
        let cmd_import_opml: PodcastCmd = PodcastCmd::load_opml_file(
            opml_path,
            PodcastCmd::process_opml_entries(vec![], PodcastCmd::end()),
        );

        let fetcher: Arc<HttpFeedFetcher> = Arc::new(HttpFeedFetcher::new());
        let mut interpreter: PodcastPipelineInterpreter =
            PodcastPipelineInterpreter::new(fetcher.clone(), podcast_event_publisher.clone());

        let initial_acc = Ok(PipelineData::default());
        let import_result: CommandAccumulator =
            run_commands(&cmd_import_opml, initial_acc, &mut interpreter).await;

        if let Err(e) = import_result {
            error!("Error: OPML import failed: {}", e);
            return Err(anyhow!(e));
        }

        if args.headless {
            info!("Headless import finished. Exiting.");
            return Ok(());
        }
    }

    let mut app: App =
        App::new(podcast_event_subscriber, player_cmd_publisher, player_event_subscriber);

    let disk_podcasts: Vec<Podcast> = load_podcasts_from_disk();

    for podcast in disk_podcasts {
        app.add_podcast(podcast);
    }

    info!("Starting player and UI tasks...");

    // An async task for handling player commands.
    // This creates a Future, it doesn't run yet.
    // This task only starts running when it is polled for the first time,
    // which happens inside the select! macro.
    let player_task = player.run();

    // A blocking task for running the entire terminal UI.
    // This moves the entire UI loop off the main async runtime, allowing
    // async tasks and the blocking UI to run concurrently without interfering with each other.
    // This starts the ui thread immediately.
    // The whole point of spawn_blocking is to get blocking work off the current thread as soon
    // as possible so it doesn't cause stalls. Delaying its start would defeat the purpose.
    let ui_task: JoinHandle<anyhow::Result<()>> = task::spawn_blocking(move || app::start_ui(app));

    // The heart of the application's concurrency model.
    // Run both tasks concurrently and wait for the first one to finish.
    // Imagine you are a manager (the Tokio select! macro). You have two workers:
    // Alice (the player_task):
    //   She's working on a complex report.
    //  You ask her for a status update. She says, "I'm waiting for an email from marketing.
    //  I'll let you know when it arrives." You move on.
    // Bob (the UI thread):
    //   You've sent him out to a construction site to supervise a long job.
    // The JoinHandle (Bob's walkie-talkie):
    //   This is the walkie-talkie you use to check on Bob.
    //  You pick it up and ask, "Bob, are you done?" He says, "Nope, still working!"
    //  You put the walkie-talkie down and move on.
    tokio::select! {
        player_result = player_task => {
            if let Err(e) = player_result {
                error!("Player task exited with error: {}", e);
            } else {
                info!("Player task finished.");
            }
        },
        ui_result = ui_task => {
            match ui_result {
                // This means the thread didn't panic,
                // and start_ui returned Ok(()), indicating a graceful exit (the user quit).
                Ok(Ok(_)) => info!("UI exited gracefully."),
                // This is returned by your start_ui function.
                // This would happen if, for example, crossterm fails to enter raw mode.
                Ok(Err(e)) => error!("UI exited with an error: {}", e),
                // his is from the JoinHandle itself.
                // It will be an Err if the thread spawned by spawn_blocking panicked.
                // The error would be a JoinError.
                Err(e) => error!("UI task panicked: {}", e),
            }
        },
    }

    Ok(())
}
