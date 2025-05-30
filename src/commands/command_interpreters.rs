// src/podcast_pipeline_interpreter.rs
use crate::commands::podcast_algebra::{CommandAccumulator, PodcastAlgebra};
use crate::errors::PipelineError;
use crate::podcast::PodcastURL;
use crate::podcast_download::HttpFeedFetcher;
use crate::podcast_download::{FeedFetcher, download_and_create_podcast};
use async_trait::async_trait;
use reqwest::Url;
use std::sync::Arc;

pub struct PodcastPipelineInterpreter {
    fetcher: Arc<dyn FeedFetcher + Send + Sync>,
}

impl PodcastPipelineInterpreter {
    pub fn new(fetcher: Arc<dyn FeedFetcher + Send + Sync>) -> Self {
        Self { fetcher }
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
        }; // Propagate error

        println!("Evaluating URL: {}", url_to_eval);

        // Step 1: Validate URL format
        let url_str = url_to_eval.as_str();
        let url = match Url::parse(url_str) {
            Ok(url) => url,
            Err(_) => {
                return Err(PipelineError::EvaluationFailed(format!(
                    "Invalid URL format: {}",
                    url_str
                )));
            }
        };

        // Step 2: Check scheme (http/https)
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(PipelineError::EvaluationFailed(format!(
                "Invalid URL scheme: {}. Only http/https supported",
                url.scheme()
            )));
        }

        // Step 3: Quick check for common RSS feed patterns
        let path = url.path().to_lowercase();
        if !path.ends_with(".xml")
            && !path.ends_with(".rss")
            && !path.contains("feed")
            && !path.contains("rss")
        {
            println!("Warning: URL doesn't follow common RSS feed patterns: {}", url_str);
        }

        // Step 4: Attempt to fetch headers to verify content type
        match self.fetcher.fetch(url_str).await {
            Ok(content) => {
                if !content.contains("<rss") && !content.contains("<feed") {
                    return Err(PipelineError::EvaluationFailed(format!(
                        "URL doesn't appear to be a valid RSS/Atom feed: {}",
                        url_str
                    )));
                }
            }
            Err(e) => {
                return Err(PipelineError::EvaluationFailed(format!(
                    "Failed to fetch URL {}: {}",
                    url_str, e
                )));
            }
        }

        println!("Evaluating URL: {}", url_to_eval);
        pipeline_data.last_evaluated_url = Some(url_to_eval.clone());
        pipeline_data.current_podcast = None; // Clear any previous podcast from context
        Ok(pipeline_data)
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

            // Step 2: Write to file (original problematic line, now fixed)
            // The `?` will work here because map_err produces PipelineError,
            // and if this function returns Result<_, PipelineError>, `?` can propagate it.
            // However, interpret_save returns CommandAccumulator (Result<PipelineData, PipelineError>),
            // so the success path of `?` needs to be `PipelineData`.
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

    async fn interpret_end(&mut self, final_acc: CommandAccumulator) -> CommandAccumulator {
        println!("Interpreter: Reached End. Final accumulator state: {:?}", final_acc);
        final_acc
    }
}
