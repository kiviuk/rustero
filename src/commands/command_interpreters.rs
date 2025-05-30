// src/podcast_pipeline_interpreter.rs
use crate::commands::podcast_algebra::{CommandAccumulator, PodcastAlgebra};
use crate::errors::PipelineError;
use crate::podcast::PodcastURL;
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

        let url_str = url_to_eval.as_str();

        println!("Interpreter: Evaluating URL (efficiently): '{}'", url_str);

        // Step 1: Basic URL parsing
        let parsed_url = match Url::parse(url_str) {
            Ok(url) => url,
            Err(_) => {
                return Err(PipelineError::EvaluationFailed(format!(
                    "Invalid URL format: {}",
                    url_str
                )));
            }
        };

        // Step 2: Check scheme (http/https)
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(PipelineError::EvaluationFailed(format!(
                "Invalid URL scheme: {}. Only http/https supported",
                parsed_url.scheme()
            )));
        }

        // Step 3: Attempt to fetch headers to verify content type
        match self.fetcher.fetch_headers(url_str).await {
            Ok(headers) => {
                if let Some(content_type) = headers.get("content-type") {
                    let ct_lower = content_type.to_lowercase();
                    if ct_lower.contains("application/rss+xml")
                        || ct_lower.contains("application/atom+xml")
                        || ct_lower.contains("application/xml")
                        || ct_lower.contains("text/xml")
                    {
                        println!("Interpreter: URL validated by Content-Type: {}", content_type);
                        pipeline_data.last_evaluated_url = Some(url_to_eval.clone());
                        pipeline_data.current_podcast = None;
                        return Ok(pipeline_data); // Early return SUCCESS
                    } else {
                        println!(
                            "Interpreter: Content-Type '{}' doesn't suggest RSS/Atom. Will try partial fetch.",
                            content_type
                        );
                    }
                } else {
                    println!("Interpreter: No Content-Type header found. Will try partial fetch.");
                }
            }
            Err(e) => {
                println!(
                    "Interpreter: HEAD request failed for {}: {}. Will try partial fetch.",
                    url_str, e
                );
                // Don't return an error yet, partial fetch is the fallback
            }
        }

        // 4. Fallback to partial GET request. This is the final validation attempt.
        //    The result of this match block will be the function's return value.
        match self.fetcher.fetch_partial_content(url_str, (0, 4095)).await {
            Ok(partial_content) => {
                println!("Interpreter: Partial content: {}", partial_content);
                if partial_content.to_lowercase().contains("<rss")
                    || partial_content.to_lowercase().contains("<feed")
                {
                    println!("Interpreter: URL validated by partial content inspection.");
                    pipeline_data.last_evaluated_url = Some(url_to_eval.clone());
                    pipeline_data.current_podcast = None;
                    Ok(pipeline_data) // SUCCESSFUL VALIDATION
                } else {
                    // DEFINITIVE FAILURE based on partial content
                    Err(PipelineError::EvaluationFailed(format!(
                        "URL content (first 4KB) of '{}' doesn't appear to be a valid RSS/Atom feed.",
                        url_str
                    )))
                }
            }
            Err(e) => {
                // DEFINITIVE FAILURE because fetching partial content failed.
                // If you have a variant like EvaluationFailedWithSource { message: String, source: DownloaderError }
                Err(PipelineError::EvaluationFailedWithSource {
                    message: format!("Failed to fetch partial content for URL '{}'", url_str),
                    source: e,
                })
            }
        }
        // No code should follow this final match expression. Its result is the function's result.
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
