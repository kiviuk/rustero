use crate::commands::podcast_algebra::{CommandAccumulator, PodcastAlgebra};
use crate::errors::{DownloaderError, PipelineError};
use crate::podcast::PodcastURL;
use crate::podcast_download::{FeedFetcher, download_and_create_podcast};
use async_trait::async_trait;
use reqwest::Url;
use std::sync::Arc;

#[derive(Debug)]
pub(super) enum ValidationStepResult {
    Validated,
    Inconclusive,
}
// Define your helper functions there as pub(super) async fn or pub(crate) async fn
// (making them accessible within the commands module or the crate).
pub(super) async fn validate_url_syntax_and_scheme(
    url_str: &str,
) -> Result<reqwest::Url, PipelineError> {
    // Step 1: Basic URL parsing
    let parsed_url = reqwest::Url::parse(url_str).map_err(|parse_err| {
        PipelineError::EvaluationFailed(format!(
            "Invalid URL format for '{}': {}",
            url_str, parse_err
        ))
    })?;

    // Step 2: Check scheme (http/https)
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err(PipelineError::EvaluationFailed(format!(
            "Invalid URL scheme for '{}': '{}'. Only http/https supported.",
            url_str,
            parsed_url.scheme()
        )));
    }
    Ok(parsed_url)
}

// Helper 3: Attempt to validate via HEAD request (Content-Type)
// Now takes `fetcher` as an argument
pub(super) async fn try_validate_via_head(
    // Changed to pub(super)
    fetcher: &(dyn FeedFetcher + Send + Sync), // Pass the fetcher trait object
    url_str: &str,
) -> Result<ValidationStepResult, DownloaderError> {
    match fetcher.fetch_headers(url_str).await {
        Ok(headers) => {
            if let Some(content_type) = headers.get("content-type") {
                let ct_lower = content_type.to_lowercase();
                if ct_lower.contains("application/rss+xml")
                    || ct_lower.contains("application/atom+xml")
                    || ct_lower.contains("application/xml")
                    || ct_lower.contains("text/xml")
                {
                    println!(
                        "Interpreter Helper (HEAD): URL validated by Content-Type: {}",
                        content_type
                    );
                    Ok(ValidationStepResult::Validated)
                } else {
                    println!(
                        "Interpreter Helper (HEAD): Content-Type '{}' inconclusive.",
                        content_type
                    );
                    Ok(ValidationStepResult::Inconclusive)
                }
            } else {
                println!("Interpreter Helper (HEAD): No Content-Type header found.");
                Ok(ValidationStepResult::Inconclusive)
            }
        }
        Err(e) => {
            println!("Interpreter Helper (HEAD): Request failed for {}: {}", url_str, e);
            Err(e)
        }
    }
}

// Helper 4: Attempt to validate via Partial GET (content sniffing)
// Fallback to partial GET request. This is the final validation attempt.
// The result of this match block will be the function's return value.
pub(super) async fn try_validate_via_partial_get(
    // Changed to pub(super)
    fetcher: &(dyn FeedFetcher + Send + Sync), // Pass the fetcher trait object
    url_str: &str,
) -> Result<ValidationStepResult, DownloaderError> {
    match fetcher.fetch_partial_content(url_str, (0, 4095)).await {
        Ok(partial_content) => {
            if partial_content.to_lowercase().contains("<rss")
                || partial_content.to_lowercase().contains("<feed")
            {
                println!("Interpreter Helper (Partial GET): URL validated by content inspection.");
                Ok(ValidationStepResult::Validated)
            } else {
                println!("Interpreter Helper (Partial GET): Content (first 4KB) inconclusive.");
                Ok(ValidationStepResult::Inconclusive)
            }
        }
        Err(e) => {
            println!("Interpreter Helper (Partial GET): Request failed for {}: {}", url_str, e);
            Err(e)
        }
    }
}
