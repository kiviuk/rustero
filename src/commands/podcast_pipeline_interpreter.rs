// src/commands/podcast_pipeline_interpreter.rs
use crate::commands::interpreter_helpers::{
    ValidationStepResult, try_validate_via_head, try_validate_via_partial_get,
    validate_url_syntax_and_scheme,
};
use crate::commands::podcast_algebra::{
    CommandAccumulator, PipelineData, PodcastAlgebra, run_commands,
};
use crate::commands::podcast_commands::PodcastCmd;
use crate::errors::PipelineError;
use crate::event::AppEvent;
use crate::opml::opml_parser::OpmlFeedEntry;
use crate::podcast::PodcastURL;
use crate::podcast_download::{FeedFetcher, download_and_create_podcast};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct PodcastPipelineInterpreter {
    fetcher: Arc<dyn FeedFetcher + Send + Sync>,
    event_tx: broadcast::Sender<AppEvent>,
}

impl PodcastPipelineInterpreter {
    pub fn new(
        fetcher: Arc<dyn FeedFetcher + Send + Sync>,
        event_tx: broadcast::Sender<AppEvent>,
    ) -> Self {
        Self { fetcher, event_tx }
    }
}

#[async_trait]
impl PodcastAlgebra for PodcastPipelineInterpreter {
    async fn interpret_eval_url(
        &mut self,
        url_to_eval: &PodcastURL,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator {
        let Ok(mut pipeline_data) = current_acc else {
            return current_acc;
        };
        let url_str = url_to_eval.as_str();
        println!("Interpreter: Evaluating URL: '{}'", url_str);

        if let Err(e) = validate_url_syntax_and_scheme(url_str).await {
            return Err(e);
        }

        // Call helper for HEAD validation
        match try_validate_via_head(self.fetcher.as_ref(), url_str).await {
            Ok(ValidationStepResult::Validated) => {
                pipeline_data.last_evaluated_url = Some(url_to_eval.clone());
                pipeline_data.current_podcast = None;
                return Ok(pipeline_data);
            }
            Ok(ValidationStepResult::Inconclusive) => {
                println!(
                    "Interpreter: HEAD validation inconclusive for {}. Proceeding to partial GET.",
                    url_str
                );
            }
            Err(head_downloader_error) => {
                println!(
                    "Interpreter: HEAD request for {} failed ({}). Proceeding to partial GET as fallback.",
                    url_str, head_downloader_error
                );
            }
        }

        match try_validate_via_partial_get(self.fetcher.as_ref(), url_str).await {
            Ok(ValidationStepResult::Validated) => {
                pipeline_data.last_evaluated_url = Some(url_to_eval.clone());
                pipeline_data.current_podcast = None;
                Ok(pipeline_data)
            }
            Ok(ValidationStepResult::Inconclusive) => {
                Err(PipelineError::EvaluationFailed(format!(
                    "URL content (first 4KB) of '{}' does not appear to be a valid RSS/Atom feed.",
                    url_str
                )))
            }
            Err(partial_get_downloader_error) => Err(PipelineError::EvaluationFailed(format!(
                "Failed to fetch partial content for URL evaluation of '{}': {}",
                url_str, partial_get_downloader_error
            ))),
        }
    }

    async fn interpret_download(
        &mut self,
        explicit_url_from_command: &PodcastURL,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator {
        let Ok(mut pipeline_data) = current_acc else {
            return current_acc;
        }; // Propagate error

        // Strategy: Use evaluated URL if available, otherwise use the one from the Download command.
        let url_to_use = match &pipeline_data.last_evaluated_url {
            Some(eval_url) => {
                println!("Interpreter: Using evaluated URL for download: {}", eval_url.as_str());
                eval_url
            }
            None => {
                println!(
                    "Interpreter: No evaluated URL in context, using URL from Download command: {}",
                    explicit_url_from_command.as_str()
                );
                explicit_url_from_command
            }
        };

        println!("Interpreter: Attempting download from: {}...", url_to_use.as_str());

        let podcast_obj = download_and_create_podcast(url_to_use, self.fetcher.as_ref()).await?; // The '?' handles the Result and early returns Err(DownloaderError) if needed

        println!("Interpreter: Successfully downloaded '{}'.", podcast_obj.title());
        pipeline_data.current_podcast = Some(podcast_obj);
        pipeline_data.last_evaluated_url = None; // "Consume" the evaluated URL
        Ok(pipeline_data)
    }

    async fn interpret_save(&mut self, current_acc: CommandAccumulator) -> CommandAccumulator {
        let Ok(data) = current_acc else {
            return current_acc;
        }; // Propagate error

        if let Some(podcast_to_save) = &data.current_podcast {
            println!(
                "Interpreter: Saving podcast (from accumulator): '{}'...",
                podcast_to_save.title()
            );

            // Step 1: Serialize (handle its potential error)
            let json_to_write = match serde_json::to_string_pretty(podcast_to_save) {
                Ok(s) => s,
                Err(serde_err) => {
                    return Err(PipelineError::SaveFailedWithSource {
                        // Use the same error variant
                        message: format!("Serialization failed for '{}'", podcast_to_save.title()),
                        source: Box::new(serde_err), // Box the serde_json::Error
                    });
                }
            };

            match std::fs::write("podcast.json", json_to_write).map_err(
                |io_error: std::io::Error| PipelineError::SaveFailedWithSource {
                    message: format!(
                        "Failed to write podcast '{}' to disk",
                        podcast_to_save.title()
                    ),
                    source: Box::new(io_error),
                },
            ) {
                Ok(_) => {
                    // fs::write succeeded
                    println!("Interpreter: Podcast '{}' saved.", podcast_to_save.title());
                    Ok(data) // Return the original PipelineData
                }
                Err(pipeline_error) => Err(pipeline_error), // fs::write failed, map_err converted it
            }
        } else {
            eprintln!("Interpreter: Save command executed, but no podcast in accumulator to save.");
            Err(PipelineError::InvalidState(
                "Save called without a podcast in accumulator".to_string(),
            ))
        }
    }

    async fn interpret_process_opml_entries(
        &mut self,
        feed_entries_to_process: &[OpmlFeedEntry],
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator {
        // Propagate error if the accumulator for ProcessOpmlEntries itself is bad
        let Ok(data) = current_acc else {
            return current_acc;
        };

        if feed_entries_to_process.is_empty() {
            println!("Interpreter: No OPML feed entries to process.");
            return Ok(data); // Nothing to do, success for this step
        }

        println!(
            "Interpreter: Processing {} OPML feed entries sequentially...",
            feed_entries_to_process.len()
        );

        for entry in feed_entries_to_process.iter() {
            let podcast_url_from_opml: PodcastURL = PodcastURL::new(&entry.xml_url);
            let command_sequence_for_entry: PodcastCmd = PodcastCmd::eval_url(
                podcast_url_from_opml.clone(),
                PodcastCmd::download(
                    podcast_url_from_opml.clone(), // Fallback URL for download
                    PodcastCmd::save(PodcastCmd::end()),
                ),
            );

            let mut sub_interpreter: PodcastPipelineInterpreter =
                PodcastPipelineInterpreter::new(self.fetcher.clone(), self.event_tx.clone());
            let initial_sub_acc: CommandAccumulator = Ok(PipelineData::default());

            let sub_result: CommandAccumulator = run_commands(
                &command_sequence_for_entry,
                initial_sub_acc,
                &mut sub_interpreter,
            )
            .await;

            if sub_result.is_err() {
                eprintln!(
                    "[OPML Processor] Sub-pipeline for {} failed: {:?}",
                    entry.title,
                    sub_result.unwrap_err()
                );
            }
        }

        Ok(data)
    }

    async fn interpret_end(&mut self, final_acc: CommandAccumulator) -> CommandAccumulator {
        println!("Interpreter: Reached End. Final accumulator state: {:?}", final_acc);
        final_acc
    }
}
