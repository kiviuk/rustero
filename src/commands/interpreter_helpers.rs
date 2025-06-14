// src/commands/interpreter_helpers.rs
use crate::errors::{DownloaderError, PipelineError};
use crate::podcast_download::FeedFetcher;
use log::{LevelFilter, info, warn, error, debug, trace}; // Import log macros

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
                    trace!(
                        "Interpreter Helper (HEAD): URL {} validated by Content-Type: {}",
                        url_str,
                        content_type
                    );
                    Ok(ValidationStepResult::Validated)
                } else {
                    trace!(
                        "Interpreter Helper (HEAD): URL {} Content-Type '{}' inconclusive.",
                        url_str,
                        content_type
                    );
                    Ok(ValidationStepResult::Inconclusive)
                }
            } else {
                info!("Interpreter Helper (HEAD): URL {} No Content-Type header found.", url_str);
                Ok(ValidationStepResult::Inconclusive)
            }
        }
        Err(e) => {
            error!("Interpreter Helper (HEAD): Request failed for URL {}: {}", url_str, e);
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
                trace!("Interpreter Helper (Partial GET): URL {} validated by content inspection.", url_str);
                Ok(ValidationStepResult::Validated)
            } else {
                trace!("Interpreter Helper (Partial GET): URL {} Content (first 4KB) inconclusive.", url_str);
                Ok(ValidationStepResult::Inconclusive)
            }
        }
        Err(e) => {
            error!("Interpreter Helper (Partial GET): Request failed for URL {}: {}", url_str, e);
            Err(e)
        }
    }
}
