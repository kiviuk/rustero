// src/podcast_download.rs
use crate::errors::DownloaderError;
use crate::podcast::{Podcast, PodcastURL};
use crate::podcast_factory::{ParsedFeed, PodcastFactory};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::{LevelFilter, debug, error, info, warn};
use reqwest::{Client, Response};
use rss::Channel;
use std::collections::HashMap; // Import log macros

#[derive(Debug, Clone)]
pub struct RawFeedData {
    pub content: String,
    pub fetch_date: DateTime<Utc>,
}

impl RawFeedData {
    pub fn from_string(content: String) -> Self {
        Self { content, fetch_date: Utc::now() }
    }
}

// ===== fetcher
#[async_trait]
pub trait FeedFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<String, DownloaderError>;

    // New method for HEAD request
    async fn fetch_headers(&self, url: &str) -> Result<HashMap<String, String>, DownloaderError>;

    // New method for partial content
    async fn fetch_partial_content(
        &self,
        url: &str,
        byte_range: (u64, u64), // e.g., (0, 4095)
    ) -> Result<String, DownloaderError>;
}

// ===== Live http fetcher
pub struct HttpFeedFetcher {
    client: Client,
}

impl HttpFeedFetcher {
    pub fn new() -> Self {
        const APP_USER_AGENT: &str = "CasteroPodcastClient/1.0\
         (+https://github.com/your-project/castero-link)\
         Mozilla/5.0 (Windows NT 10.0; Win64; x64)\
          AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36";

        let client: Client = reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create request client.");

        Self { client }
    }
}

#[async_trait]
impl FeedFetcher for HttpFeedFetcher {
    async fn fetch(&self, url: &str) -> Result<String, DownloaderError> {
        info!("HttpFeedFetcher: fetching {}", url);
        Ok(self
            .client
            .get(url)
            .send()
            .await
            .map_err(DownloaderError::NetworkError)?
            .text()
            .await
            .map_err(DownloaderError::NetworkError)?)
    }

    async fn fetch_headers(&self, url: &str) -> Result<HashMap<String, String>, DownloaderError> {
        debug!("HttpFeedFetcher: fetching HEAD for {}", url);
        let response: Response =
            self.client.head(url).send().await.map_err(DownloaderError::NetworkError)?;
        if !response.status().is_success() {
            return Err(DownloaderError::Failed(format!(
                "HEAD request failed with status: {}",
                response.status()
            )));
        }
        let mut headers_map: HashMap<String, String> = HashMap::new();
        for (key, value) in response.headers().iter() {
            if let Ok(value_str) = value.to_str() {
                headers_map.insert(key.as_str().to_lowercase(), value_str.to_string());
            }
        }
        Ok(headers_map)
    }

    async fn fetch_partial_content(
        &self,
        url: &str,
        byte_range: (u64, u64),
    ) -> Result<String, DownloaderError> {
        debug!("HttpFeedFetcher: fetching partial content for {}", url);
        let response: Response = self
            .client
            .get(url)
            .header("Range", format!("bytes={}-{}", byte_range.0, byte_range.1))
            .send()
            .await
            .map_err(DownloaderError::NetworkError)?;

        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(DownloaderError::Failed(format!(
                "Partial GET request failed with status: {}",
                response.status()
            )));
        }
        response.text().await.map_err(DownloaderError::NetworkError)
    }
}

// ===== Fake http fetcher for testing
pub struct FakeFetcher {
    pub response: String,
}

#[async_trait]
impl FeedFetcher for FakeFetcher {
    async fn fetch(&self, _url: &str) -> Result<String, DownloaderError> {
        Ok(self.response.clone())
    }

    // New method for HEAD request

    async fn fetch_headers(&self, _url: &str) -> Result<HashMap<String, String>, DownloaderError> {
        // Return some fake headers, e.g., based on self.response for testing
        let mut headers: HashMap<String, String> = HashMap::new();
        if self.response.contains("<rss") || self.response.contains("<feed") {
            headers.insert("content-type".to_string(), "application/xml".to_string());
        } else {
            headers.insert("content-type".to_string(), "text/html".to_string());
        }
        Ok(headers)
    }

    // For partial content
    async fn fetch_partial_content(
        &self,
        _url: &str,
        byte_range: (u64, u64),
    ) -> Result<String, DownloaderError> {
        let start: usize = byte_range.0 as usize;
        let end: usize = (byte_range.1 + 1) as usize; // Range is inclusive, slice is exclusive at end
        if start < self.response.len() {
            let effective_end: usize = std::cmp::min(end, self.response.len());
            Ok(self.response[start..effective_end].to_string())
        } else {
            Ok("".to_string())
        }
    }
}

// Implementation of the download function
pub async fn download_and_create_podcast(
    url: &PodcastURL,
    fetcher: &(dyn FeedFetcher + Send + Sync),
) -> Result<Podcast, DownloaderError> {
    info!("download_and_create_podcast: Fetching content for URL: {}", url.as_str());
    let content: String = fetcher.fetch(url.as_str()).await?;
    info!("download_and_create_podcast: Content fetched, length: {}", content.len());
    let channel: Channel = Channel::read_from(content.as_bytes())?;
    let parsed = ParsedFeed { channel };

    PodcastFactory::new().create_podcast(parsed, url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcast::PodcastURL;

    #[tokio::test]
    async fn test_download_and_create_podcast() {
        // Create a dummy RSS feed content
        let dummy_feed: String = r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <rss version="2.0">
                <channel>
                    <title>Test Podcast</title>
                    <link>http://example.com/feed</link>
                    <description>Test Description</description>
                    <image>
                        <url>http://example.com/image.jpg</url>
                    </image>
                </channel>
            </rss>
        "#
        .to_string();

        let fetcher = FakeFetcher { response: dummy_feed };

        let url: PodcastURL = PodcastURL::new("http://example.com/feed");
        let podcast: Podcast = download_and_create_podcast(&url, &fetcher).await.unwrap();

        assert_eq!(podcast.title(), "Test Podcast");
        assert_eq!(podcast.url().as_str(), url.as_str());
        assert_eq!(podcast.description(), Some("Test Description"));
        assert_eq!(podcast.website_url(), Some(url.as_str()));
    }

    #[tokio::test]
    async fn test_real_feed_download() {
        let fetcher: HttpFeedFetcher = HttpFeedFetcher::new();
        let url: PodcastURL = PodcastURL::new("https://feeds.zencastr.com/f/oSn1i316.rss");

        let podcast: Podcast = download_and_create_podcast(&url, &fetcher).await.unwrap();

        info!("Downloaded podcast: {:#?}", podcast);

        // Basic sanity checks
        assert_eq!(podcast.title(), "Developer Voices");
        assert_eq!(podcast.url(), &url);
        assert!(podcast.description().is_some());
        assert!(podcast.image_url().is_some());
        assert_eq!(podcast.website_url(), Some("http://www.developervoices.com"));
    }

    // SAD PATHS

    #[tokio::test]
    async fn test_malformed_feed() {
        let malformed_xml: &str = r#"<?xml version="1.0"?><rss><channel>"#;
        let fetcher = FakeFetcher { response: malformed_xml.to_string() };

        let result: Result<Podcast, DownloaderError> =
            download_and_create_podcast(&PodcastURL::new("http://example.com"), &fetcher).await;
        assert!(matches!(result, Err(DownloaderError::RssError(_))));
    }
}
