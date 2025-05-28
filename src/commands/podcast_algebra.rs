// src/podcast_cmd.rs (continued)
use crate::errors::{PipelineError, PodcastError};
use crate::podcast::{EpisodeID, Podcast, PodcastURL};

use async_trait::async_trait;
use crate::commands::podcast_commands::PodcastCmd;

#[derive(Debug, Clone, Default)]
pub struct PipelineData {
    pub last_evaluated_url: Option<PodcastURL>, // Result from EvalUrl
    pub current_podcast: Option<Podcast>,     // Result from Download
    // Add other fields as pipeline evolves
}

// The Accumulator type that will be threaded through
pub type CommandAccumulator = Result<PipelineData, PipelineError>;

impl PipelineData {
    // Creates a new PipelineData ensuring it's only valid if the previous result was Ok
    fn from_previous_ok(prev_res: &CommandAccumulator) -> Option<Self> {
        match prev_res {
            Ok(data) => Some(data.clone()), // Clone the data to modify
            Err(_) => None, // If previous was error, can't proceed with data
        }
    }
}

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

    async fn interpret_end(
        &mut self,
        final_acc: CommandAccumulator,
    ) -> CommandAccumulator;
}
pub async fn run_commands(
    command: &PodcastCmd,
    initial_accumulator: CommandAccumulator,
    algebra: &mut impl PodcastAlgebra,
) -> CommandAccumulator {
    let mut current_acc = initial_accumulator;
    let mut current_cmd_node = command;

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
            PodcastCmd::End => {
                current_acc = algebra.interpret_end(current_acc).await;
                break; // Exit the loop
            }
        }
    }
    current_acc
}