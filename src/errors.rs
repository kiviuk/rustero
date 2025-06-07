// src/errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PodcastError {
    #[error("Feed parsing error: {0}")]
    ParseError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid feed URL: {0}")]
    InvalidUrl(String),

    #[error("Feed too large: {size} bytes")]
    FeedTooLarge { size: usize },

    #[error("Failed to save podcast url: {0}")]
    SaveFailed(String), // Store the URL as a string
}

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error), // For fetcher.fetch if it uses reqwest directly
    #[error("RSS parsing error: {0}")]
    RssError(#[from] rss::Error), // For rss::Channel::read_from
    #[error("Download failed: {0}")]
    Failed(String),
}

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Download operation failed: {0}")]
    DownloadFailed(#[from] DownloaderError),
    #[error("Save operation failed: {0}")]
    SaveFailedWithMessage(String),
    #[error("Save operation failed with underlying cause: {source}")]
    SaveFailedWithSource {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("URL evaluation failed: {0}")]
    EvaluationFailed(String),
    #[error("Evaluation the url failed with underlying cause: {source}")]
    EvaluationFailedWithSource {
        message: String,
        #[source]
        source: DownloaderError,
    },
    #[error("Pipeline is in an invalid state: {0}")]
    InvalidState(String), // e.g., Save called when no podcast in context
    #[error("An earlier step in the pipeline failed: {0}")] // {0} will display source
    UpstreamError(#[from] Box<PipelineError>),
}
