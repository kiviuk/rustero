// errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PodcastError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Feed parsing error: {0}")]
    ParseError(String),

    #[error("RSS parsing error: {0}")]
    RssError(#[from] rss::Error),

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
pub enum DownloaderError {}


#[derive(Debug)]
pub enum PipelineError {
    DownloadFailed(DownloaderError),
    SaveFailed(String),
    EvaluationFailed(String),
    InvalidState(String), // e.g., Save called when no podcast in context
    UpstreamError(Box<PipelineError>), // To wrap an error from a previous step
}

// Optional: Implement std::error::Error and std::fmt::Display for PipelineError
impl std::fmt::Display for PipelineError { 
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{:?}", self) } 
}
impl std::error::Error for PipelineError {}

// If DownloaderError is distinct and needs conversion
impl From<DownloaderError> for PipelineError {
    fn from(err: DownloaderError) -> Self {
        PipelineError::DownloadFailed(err)
    }
}