// src/commands/podcast_pipeline_interpreter.rs
use crate::commands::interpreter_helpers::{
    ValidationStepResult, try_validate_via_head, try_validate_via_partial_get,
    validate_url_syntax_and_scheme,
};
use crate::commands::podcast_algebra::{
    CommandAccumulator, PipelineData, PodcastAlgebra, run_commands,
};
use crate::commands::podcast_commands::PodcastCmd;
use crate::errors::{DownloaderError, PipelineError}; // Import DownloaderError
use crate::event::AppEvent;
use crate::opml::opml_parser::{OpmlFeedEntry, parse_opml_from_file}; // Import parse_opml_from_file
use crate::podcast::{Podcast, PodcastURL};
use crate::podcast_download::{FeedFetcher, download_and_create_podcast};
use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf; // For constructing paths
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

pub const PODCAST_DATA_DIR: &str = "podcast_data";

// Helper function to calculate a hash for a given string
fn calculate_url_hash(url_str: &str) -> String {
    let mut s = DefaultHasher::new();
    url_str.hash(&mut s);
    format!("{:x}", s.finish()) // Return as hex string
}

// Helper function to generate the podcast filename before saving to disk
fn generate_podcast_filename(podcast_url: &PodcastURL) -> Result<String, PipelineError> {
    let url_str = podcast_url.as_str();
    let parsed_url = url::Url::parse(url_str).map_err(|parse_err| {
        PipelineError::SaveFailedWithMessage(format!(
            "Invalid URL format for filename generation ('{}'): {}",
            url_str, parse_err
        ))
    })?;

    let host = parsed_url.host_str().unwrap_or("unknown_host").to_string();
    // Basic sanitization for host: replace characters not ideal for filenames
    // More robust sanitization might be needed depending on expected hostnames
    let sanitized_host = host.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-', "_");

    let url_hash = calculate_url_hash(url_str);

    Ok(format!("{}-{}.json", sanitized_host, url_hash))
}

#[async_trait]
impl PodcastAlgebra for PodcastPipelineInterpreter {
    async fn interpret_eval_url(
        &mut self,
        url_to_eval: &PodcastURL,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator {
        let Ok(mut pipeline_data): CommandAccumulator = current_acc else {
            return current_acc;
        };
        let url_str: &str = url_to_eval.as_str();
        println!("Interpreter: Evaluating URL: '{}'", url_str);

        // Basic validation by URL syntax and scheme
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

        // Final fallback: Partial GET validation
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
        let Ok(mut pipeline_data): CommandAccumulator = current_acc else {
            return current_acc;
        }; // Propagate error

        // Strategy: Use evaluated URL if available, otherwise use the one from the Download command.
        let url_to_use: &PodcastURL = match &pipeline_data.last_evaluated_url {
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

        // The '?' handles the Result and early returns Err(DownloaderError) if needed
        let podcast_obj: Podcast =
            download_and_create_podcast(url_to_use, self.fetcher.as_ref()).await?;

        println!("Interpreter: Successfully downloaded '{}'.", podcast_obj.title());
        pipeline_data.current_podcast = Some(podcast_obj);
        pipeline_data.last_evaluated_url = None; // "Consume" the evaluated URL
        Ok(pipeline_data)
    }

    async fn interpret_save(&mut self, current_acc: CommandAccumulator) -> CommandAccumulator {
        let Ok(data): CommandAccumulator = current_acc else {
            return current_acc;
        }; // Propagate error

        if let Some(podcast_to_save) = &data.current_podcast {
            println!(
                "Interpreter: Saving podcast (from accumulator): '{}'...",
                podcast_to_save.title()
            );

            // Generate the filename
            let filename: String = match generate_podcast_filename(podcast_to_save.url()) {
                Ok(name) => name,
                Err(e) => return Err(e),
            };

            // Ensure the data directory exists
            if let Err(io_err) = fs::create_dir_all(PODCAST_DATA_DIR) {
                return Err(PipelineError::SaveFailedWithSource {
                    message: format!(
                        "Failed to create podcast data directory '{}'",
                        PODCAST_DATA_DIR
                    ),
                    source: Box::new(io_err),
                });
            }

            let file_path: PathBuf = PathBuf::from(PODCAST_DATA_DIR).join(filename);

            // Serialize the podcast
            let json_to_write: String = match serde_json::to_string_pretty(podcast_to_save) {
                Ok(s) => s,
                Err(serde_err) => {
                    return Err(PipelineError::SaveFailedWithSource {
                        message: format!("Serialization failed for '{}'", podcast_to_save.title()),
                        source: Box::new(serde_err),
                    });
                }
            };

            // Write to the specific file
            match fs::write(&file_path, json_to_write) {
                Ok(_) => {
                    println!(
                        "Interpreter: Podcast '{}' saved to '{}'.",
                        podcast_to_save.title(),
                        file_path.display()
                    );
                    // Emit an event that a podcast is ready for the app
                    // (if it wasn't already emitted or if saving is the definitive step)
                    // For now, we assume Download might have already prepared it, but saving confirms it.
                    // If you want App to only pick up *saved* podcasts, this is where you'd send the event.
                    // For example:
                    if let Err(e) = self.event_tx.send(AppEvent::PodcastReadyForApp {
                        podcast: podcast_to_save.clone(), // Clone the podcast data for the event
                        timestamp: chrono::Utc::now(),
                    }) {
                        eprintln!("Failed to send PodcastReadyForApp event after save: {}", e);
                        // Decide how to handle this error; for now, just log it.
                        // It might mean the app's receiver is gone.
                    }

                    Ok(data) // Return the original PipelineData
                }
                Err(io_error) => Err(PipelineError::SaveFailedWithSource {
                    message: format!(
                        "Failed to write podcast '{}' to disk at '{}'",
                        podcast_to_save.title(),
                        file_path.display()
                    ),
                    source: Box::new(io_error),
                }),
            }
        } else {
            eprintln!("Interpreter: Save command executed, but no podcast in accumulator to save.");
            Err(PipelineError::InvalidState(
                "Save called without a podcast in accumulator".to_string(),
            ))
        }
    }
    
    async fn interpret_load_opml_file(
        &mut self,
        file_path: &PathBuf,
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator {
        let Ok(mut pipeline_data): CommandAccumulator = current_acc else {
            return current_acc;
        };
        println!("Interpreter: Loading OPML file from: {}", file_path.display());

        let entries = parse_opml_from_file(file_path)
            .map_err(|e| PipelineError::EvaluationFailedWithSource {
                message: format!("Failed to parse OPML file '{}': {}", file_path.display(), e),
                source: DownloaderError::Failed(e.to_string()), // Wrap OpmlParseError
            })?;

        println!(
            "Interpreter: Successfully loaded {} OPML entries from {}",
            entries.len(),
            file_path.display()
        );
        pipeline_data.opml_entries = Some(entries);
        Ok(pipeline_data)
    }
    
    async fn interpret_process_opml_entries(
        &mut self,
        feed_entries_to_process: &[OpmlFeedEntry],
        current_acc: CommandAccumulator,
    ) -> CommandAccumulator {
        let mut data: PipelineData = match current_acc {
            Ok(d) => d,
            Err(_) => return current_acc,
        };

        let feed_entries_to_process: Vec<OpmlFeedEntry> = if let Some(acc_entries) = data.opml_entries.take() {
            // Use entries from the accumulator and consume them
            acc_entries
        } else {
            // Fallback to entries passed directly in the command (e.g., if called directly)
            feed_entries_to_process.to_vec()
        };

        // This would unwrap Ok(data) or return Err(e)
        //let data: PipelineData = current_acc?;

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

            let sub_result: CommandAccumulator =
                run_commands(&command_sequence_for_entry, initial_sub_acc, &mut sub_interpreter)
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
