// src/main.rs
use anyhow::anyhow;
use clap::Parser;
use rustero::app::{self, App, load_podcasts_from_disk};
use rustero::commands::podcast_algebra::{run_commands, PipelineData};
use rustero::commands::podcast_commands::PodcastCmd;
use rustero::commands::podcast_pipeline_interpreter::PodcastPipelineInterpreter;
use rustero::event::AppEvent;
use rustero::player::{AudioPlayer, PlayerCommand, PlayerEvent};
use rustero::podcast_download::HttpFeedFetcher;
use log::{error, info, LevelFilter};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::sync::mpsc;
// --- ADDED: The tool to solve the blocking problem ---
use tokio::task;

fn setup_logger() -> anyhow::Result<()> {
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
    #[arg(long, value_name = "FILE")]
    import_opml_file: Option<PathBuf>,
    #[arg(long)]
    headless: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = Args::parse();
    setup_logger()?;

    let (event_tx_main, app_event_rx): (Sender<AppEvent>, Receiver<AppEvent>) =
        broadcast::channel::<AppEvent>(32);

    let (player_cmd_tx, player_cmd_rx) = mpsc::channel::<PlayerCommand>(100);
    let (player_event_tx, player_event_rx) = broadcast::channel::<PlayerEvent>(100);

    // AudioPlayer is created here, on the main tokio thread. It is NOT Send.
    let mut player = AudioPlayer::new(player_cmd_rx, player_event_tx)
        .expect("Failed to create AudioPlayer");

    if let Some(opml_path) = args.import_opml_file {
        info!("--- Processing OPML import from: {} ---", opml_path.display());
        let cmd_import_opml = PodcastCmd::load_opml_file(
            opml_path,
            PodcastCmd::process_opml_entries(vec![], PodcastCmd::end()),
        );

        let fetcher = Arc::new(HttpFeedFetcher::new());
        let mut interpreter =
            PodcastPipelineInterpreter::new(fetcher.clone(), event_tx_main.clone());

        let initial_acc = Ok(PipelineData::default());
        let import_result = run_commands(&cmd_import_opml, initial_acc, &mut interpreter).await;

        if let Err(e) = import_result {
            error!("Error: OPML import failed: {}", e);
            return Err(anyhow!(e));
        }

        if args.headless {
            info!("Headless import finished. Exiting.");
            return Ok(());
        }
    }

    // The App struct IS Send, so it can be moved to the blocking thread.
    let mut app: App = App::new(app_event_rx, player_cmd_tx, player_event_rx);
    let disk_podcasts = load_podcasts_from_disk();
    for podcast in disk_podcasts {
        app.add_podcast(podcast);
    }
    
    info!("Starting player and UI tasks...");

    // --- THIS IS THE CORRECT ARCHITECTURE ---
    // The player runs on the async runtime.
    let player_task = player.run();
    // The entire blocking UI runs on a dedicated thread from Tokio's pool.
    let ui_task = task::spawn_blocking(move || app::start_ui(app));

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
                Ok(Ok(_)) => info!("UI exited gracefully."),
                Ok(Err(e)) => error!("UI exited with an error: {}", e),
                Err(e) => error!("UI task panicked: {}", e),
            }
        },
    }

    Ok(())
}