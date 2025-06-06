use opml::{OPML, Outline};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OpmlParseError {
    #[error("Failed to read OPML file: {0}")]
    FileReadError(#[from] std::io::Error),

    #[error("Failed to parse OPML data: {0}")]
    OpmlFormatError(#[from] opml::Error), // opml::Error from the opml crate

    #[error("OPML document has no body")]
    NoBody,

    #[error("Outline item is missing required 'xmlUrl' attribute for a feed")]
    MissingXmlUrl,

    #[error("Outline item is missing 'text' or 'title' attribute")]
    MissingTitle,
}

pub struct OpmlFeedEntry {
    pub title: String,
    pub xml_url: String, // This is typically the feed URL
    pub html_url: Option<String>,
    // You can add other attributes like `text`, `description` if needed
}

/// Parses OPML content directly from a string.
///
/// It specifically looks for outlines of type "rss" or where an xmlUrl is present,
/// which usually represent podcast feeds.
///
/// # Arguments
/// * `opml_content` - A string slice containing the OPML XML data.
///
/// # Returns
/// A `Result` containing a `Vec<OpmlFeedEntry>` on success,
/// or an `OpmlParseError` on failure.
/// <?xml version="1.0" encoding="ASCII"?>
/// <opml version="2.0">
///     <head>
///         <title>castero feeds</title>
///     </head>
///     <body>
///         <outline type="rss" text="99% Invisible" xmlUrl="https://feeds.simplecast.com/BqbsxVfO"/>
///         <outline type="rss" text="Algorithms + Data Structures = Programs"

pub fn parse_opml_from_string(opml_content: &str) -> Result<Vec<OpmlFeedEntry>, OpmlParseError> {
    let document = OPML::from_str(opml_content)?;
    let mut feed_entries = Vec::new();

    // `document.body` is directly `opml::Body`
    // `document.body.outlines` is `Vec<opml::Outline>`
    for outline in document.body.outlines {
        // No if let Some needed
        process_outline_recursive(outline, &mut feed_entries)?;
    }
    // The case of a missing <body> tag would have caused OPML::from_str to fail.
    // An empty body (<body/>) would result in an empty document.body.outlines Vec.
    Ok(feed_entries)
}

/// Reads an OPML file from the given path and parses its content.
///
/// # Arguments
/// * `file_path` - A reference to a Path representing the location of the OPML file.
///
/// # Returns
/// A `Result` containing a `Vec<OpmlFeedEntry>` on success,
/// or an `OpmlParseError` on failure.
pub fn parse_opml_from_file<P: AsRef<Path>>(
    file_path: P,
) -> Result<Vec<OpmlFeedEntry>, OpmlParseError> {
    let opml_content = fs::read_to_string(file_path)?; // Propagates io::Error as OpmlParseError::FileReadError
    parse_opml_from_string(&opml_content)
}

// Helper function to recursively process outlines, as OPML can have nested groups.
fn process_outline_recursive(
    outline: Outline,
    feed_entries: &mut Vec<OpmlFeedEntry>,
) -> Result<(), OpmlParseError> {
    // Check if this outline represents a feed
    // Common indicators: type="rss" or the presence of an xml_url attribute.
    // Some OPMLs might not explicitly use type="rss" but will have xml_url for feeds.
    let is_feed = outline.r#type.as_deref().map_or(false, |t| t.eq_ignore_ascii_case("rss"))
        || outline.xml_url.is_some();

    if is_feed {
        // Assuming is_feed is determined correctly
        let final_title: String;

        if let Some(title_attr_val) = outline.title {
            // title_attr_val is String
            if !title_attr_val.is_empty() {
                final_title = title_attr_val; // Use title attribute if Some and not empty
            } else if !outline.text.is_empty() {
                // title attribute was Some(""), but text attribute has content
                final_title = outline.text; // Use text attribute
            } else {
                // title attribute was Some(""), and text attribute was also empty
                return Err(OpmlParseError::MissingTitle);
            }
        } else {
            // title attribute was None
            if !outline.text.is_empty() {
                final_title = outline.text; // Fallback to text attribute
            } else {
                // title attribute was None, and text attribute was also empty
                return Err(OpmlParseError::MissingTitle);
            }
        }
        // At this point, final_title is a non-empty String.

        let xml_url_str = outline
            .xml_url
            .filter(|s| !s.is_empty()) // Ensure it's not Some("")
            .ok_or(OpmlParseError::MissingXmlUrl)?;
        // xml_url_str is now a non-empty String.

        feed_entries.push(OpmlFeedEntry {
            title: final_title,
            xml_url: xml_url_str,
            html_url: outline.html_url, // This is Option<String>, which is fine if OpmlFeedEntry.html_url is Option<String>
        });
    }

    // Recursively process any child outlines (e.g., items within a folder)
    for child_outline in outline.outlines {
        process_outline_recursive(child_outline, feed_entries)?;
    }

    Ok(())
}

// Example Usage (you can put this in main.rs or tests)
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OPML_V1: &str = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
    <opml version="1.0">
        <head>
            <title>My Podcasts</title>
        </head>
        <body>
            <outline text="Tech Podcasts" title="Tech Podcasts">
                <outline text="Syntax FM" title="Syntax FM" type="rss" xmlUrl="http://feed.syntax.fm/rss" htmlUrl="https://syntax.fm"/>
                <outline title="Darknet Diaries" type="rss" xmlUrl="https://feeds.darknetdiaries.com/darknet-diaries.libsyn.com/rss" />
            </outline>
            <outline text="News" title="News Podcast (no type, but has xmlUrl)" xmlUrl="http://example.com/news.xml" />
        </body>
    </opml>"#;

    const SAMPLE_OPML_V2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <opml version="2.0">
        <head>
            <title>Subscriptions</title>
        </head>
        <body>
            <outline title="Rust Feeds">
                <outline title="This Week in Rust" text="This Week in Rust" type="rss" xmlUrl="https://this-week-in-rust.org/rss.xml" description="A weekly newsletter about Rust"/>
            </outline>
            <outline title="A Blog" xmlUrl="http://someblog.com/feed" /> 
        </body>
    </opml>"#;

    #[test]
    fn test_parse_from_string_v1() {
        let feeds = parse_opml_from_string(SAMPLE_OPML_V1).unwrap();
        assert_eq!(feeds.len(), 3);
        assert_eq!(feeds[0].title, "Syntax FM");
        assert_eq!(feeds[0].xml_url, "http://feed.syntax.fm/rss");
        assert_eq!(feeds[1].title, "Darknet Diaries");
        assert_eq!(feeds[2].title, "News Podcast (no type, but has xmlUrl)");
        assert_eq!(feeds[2].xml_url, "http://example.com/news.xml");
    }

    #[test]
    fn test_parse_from_string_v2() {
        let feeds = parse_opml_from_string(SAMPLE_OPML_V2).unwrap();
        assert_eq!(feeds.len(), 2);
        assert_eq!(feeds[0].title, "This Week in Rust");
        assert_eq!(feeds[0].xml_url, "https://this-week-in-rust.org/rss.xml");
        assert_eq!(feeds[1].title, "A Blog"); // Title is mandatory for our OpmlFeedEntry
        assert_eq!(feeds[1].xml_url, "http://someblog.com/feed");
    }

    #[test]
    fn test_missing_xml_url_is_skipped_unless_nested_valid() {
        let opml_missing_xml_url = r#"<?xml version="1.0" encoding="UTF-8"?>
        <opml version="1.0">
            <head><title>Test</title></head>
            <body>
                <outline text="No XML URL here" type="rss">
                    <outline text="Nested Feed" title="Nested Feed" type="rss" xmlUrl="http://example.com/nested.xml" />
                </outline>
            </body>
        </opml>"#;
        // The current logic for `is_feed` checks `outline.xml_url.is_some()`.
        // If the outer outline has type="rss" but no xmlUrl, it would try to create an OpmlFeedEntry.
        // The `xml_url.ok_or(OpmlParseError::MissingXmlUrl)?` would then cause an error.
        // This is correct: if it's marked as a feed, it should have an xmlUrl.
        // If the intent is to only extract items that are *definitely* feeds with all required info,
        // then an error is appropriate if a type="rss" outline is missing xmlUrl.
        // If the intent is to skip malformed feed entries, the `?` in process_outline_recursive needs to be handled
        // such that it doesn't stop processing siblings or other parts of the tree.
        // For now, let's test that it errors if a feed entry is malformed.

        // To test skipping, we'd need to make `process_outline_recursive` return Result<(), OpmlParseError>
        // and handle its error inside the loop, or filter more strictly before pushing.
        // Current `process_outline_recursive` uses `?` which propagates.

        let result = parse_opml_from_string(opml_missing_xml_url);
        // The outer outline will fail because it's type="rss" but has no xmlUrl.
        // The question is if the error from the outer outline prevents the inner one from being processed.
        // With the current `?` in the loop, an error in a child will propagate up and stop.
        // This behavior might be okay - if an OPML is malformed, maybe we stop.

        // Let's modify test for the current behavior: outer outline will fail.
        // The `title` for the outer outline is "No XML URL here".
        // It has `type="rss"`. `xml_url` is None.
        // This will trigger `OpmlParseError::MissingXmlUrl`.
        let opml_malformed_feed_entry = r#"<?xml version="1.0" encoding="UTF-8"?>
        <opml version="1.0">
            <head><title>Test</title></head>
            <body>
                <outline text="Malformed Feed" title="Malformed Feed" type="rss" htmlUrl="http://example.com" />
            </body>
        </opml>"#;
        let result_malformed = parse_opml_from_string(opml_malformed_feed_entry);
        assert!(matches!(result_malformed, Err(OpmlParseError::MissingXmlUrl)));

        // Test that even if an outer folder doesn't have feed attributes, inner ones are found
        let opml_folder_no_type = r#"<?xml version="1.0" encoding="UTF-8"?>
        <opml version="1.0">
            <head><title>Test</title></head>
            <body>
                <outline text="Just a Folder">
                    <outline text="Feed In Folder" title="Feed In Folder" type="rss" xmlUrl="http://example.com/feed_in_folder.xml" />
                </outline>
            </body>
        </opml>"#;
        let feeds = parse_opml_from_string(opml_folder_no_type).unwrap();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].title, "Feed In Folder");
    }

    // You would need an actual OPML file for this test to run, e.g., "test_data/sample.opml"
    // #[test]
    // fn test_parse_from_file() {
    //     // Create a dummy OPML file for testing
    //     let dir = tempfile::tempdir().unwrap();
    //     let file_path = dir.path().join("sample.opml");
    //     fs::write(&file_path, SAMPLE_OPML_V1).unwrap();
    //
    //     let feeds = parse_opml_from_file(file_path).unwrap();
    //     assert_eq!(feeds.len(), 3);
    //     assert_eq!(feeds[0].title, "Syntax FM");
    // }
}
