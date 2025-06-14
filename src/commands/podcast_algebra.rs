use std::path::PathBuf;
// src/commands/podcast_cmd.rs (continued)
use crate::errors::PipelineError;
use crate::podcast::{Podcast, PodcastURL};

use crate::commands::podcast_commands::PodcastCmd;
use crate::opml::opml_parser::OpmlFeedEntry;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct PipelineData {
    pub last_evaluated_url: Option<PodcastURL>, // Result from EvalUrl
    pub current_podcast: Option<Podcast>,       // Result from Download
    pub opml_entries: Option<Vec<OpmlFeedEntry>>, // New: For passing parsed OPML entries
}

// The Accumulator type that will be threaded through
pub type CommandAccumulator = Result<PipelineData, PipelineError>;

#[async_trait]
pub trait PodcastAlgebra {
    async fn interpret_eval_url(
        &mut self,
        url_to_eval: &PodcastURL,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator;

    async fn interpret_download(
        &mut self,
        // URL explicitly provided by the Download command node
        explicit_url_from_command: &PodcastURL,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator;

    async fn interpret_save(
        &mut self,
        // Save implicitly uses data from the accumulator
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator;
    // New trait method for loading OPML file
    async fn interpret_load_opml_file(
        &mut self,
        file_path: &PathBuf,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator;

    async fn interpret_process_opml_entries(
        &mut self,
        feed_entries_to_process: &[OpmlFeedEntry],
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator;

    async fn interpret_end(&mut self, final_acc: CommandAccumulator) -> CommandAccumulator;
}
pub async fn run_commands(
    command: &PodcastCmd,
    initial_accumulator: CommandAccumulator,
    algebra: &mut impl PodcastAlgebra,
) -> CommandAccumulator {
    let mut current_acc: CommandAccumulator = initial_accumulator;
    let mut current_cmd_node: &PodcastCmd = command;

    loop {
        // Algebra methods are responsible for checking current_acc.is_err()
        // and propagating the error if they don't intend to handle/recover it.
        match current_cmd_node {
            PodcastCmd::EvalUrl(url, next_cmd) => {
                current_acc = algebra.interpret_eval_url(url, current_acc).await;
                current_cmd_node = next_cmd;
            }
            PodcastCmd::Download(url, next_cmd) => {
                current_acc = algebra.interpret_download(url, current_acc).await;
                current_cmd_node = next_cmd;
            }
            PodcastCmd::Save(next_cmd) => {
                current_acc = algebra.interpret_save(current_acc).await;
                current_cmd_node = next_cmd;
            }
            PodcastCmd::LoadOpmlFile(path, next_cmd) => {
                current_acc = algebra.interpret_load_opml_file(path, current_acc).await;
                current_cmd_node = next_cmd;
            }
            PodcastCmd::ProcessOpmlEntries(location, next_cmd) => {
                current_acc = algebra.interpret_process_opml_entries(&location, current_acc).await;
                current_cmd_node = next_cmd;
            }

            PodcastCmd::End => {
                current_acc = algebra.interpret_end(current_acc).await;
                break; // Exit the loop
            }
        }
    }
    current_acc
}
