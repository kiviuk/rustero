// src/podcast_download.rs
use crate::errors::DownloaderError;
use crate::podcast::{Podcast, PodcastURL};
use crate::podcast_factory::{ParsedFeed, PodcastFactory};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

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
    client: reqwest::Client,
}

impl HttpFeedFetcher {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

#[async_trait]
impl FeedFetcher for HttpFeedFetcher {
    async fn fetch(&self, url: &str) -> Result<String, DownloaderError> {
        println!("HttpFeedFetcher: fetching {}", url);
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
        let response = self.client.head(url).send().await.map_err(DownloaderError::NetworkError)?;
        if !response.status().is_success() {
            return Err(DownloaderError::Failed(format!(
                "HEAD request failed with status: {}",
                response.status()
            )));
        }
        let mut headers_map = HashMap::new();
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
        let response = self
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
        let mut headers = HashMap::new();
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
        let start = byte_range.0 as usize;
        let end = (byte_range.1 + 1) as usize; // Range is inclusive, slice is exclusive at end
        if start < self.response.len() {
            let effective_end = std::cmp::min(end, self.response.len());
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
    println!("download_and_create_podcast: Fetching content for URL: {}", url.as_str());
    let content = fetcher.fetch(url.as_str()).await?;
    println!("download_and_create_podcast: Content fetched, length: {}", content.len());
    let channel = rss::Channel::read_from(content.as_bytes())?;
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
        let dummy_feed = r#"
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

        let url = PodcastURL::new("http://example.com/feed");
        let podcast = download_and_create_podcast(&url, &fetcher).await.unwrap();

        assert_eq!(podcast.title(), "Test Podcast");
        assert_eq!(podcast.url().as_str(), url.as_str());
        assert_eq!(podcast.description(), Some("Test Description"));
        assert_eq!(podcast.website_url(), Some(url.as_str()));
    }

    #[tokio::test]
    async fn test_real_feed_download() {
        let fetcher = HttpFeedFetcher::new();
        let url = PodcastURL::new("https://feeds.zencastr.com/f/oSn1i316.rss");

        let podcast = download_and_create_podcast(&url, &fetcher).await.unwrap();

        println!("Downloaded podcast: {:#?}", podcast);

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
        let malformed_xml = r#"<?xml version="1.0"?><rss><channel>"#;
        let fetcher = FakeFetcher { response: malformed_xml.to_string() };

        let result =
            download_and_create_podcast(&PodcastURL::new("http://example.com"), &fetcher).await;
        assert!(matches!(result, Err(DownloaderError::RssError(_))));
    }
}
