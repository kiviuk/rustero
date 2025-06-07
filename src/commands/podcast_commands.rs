// src/commands/podcast_commands.rs
use crate::opml::opml_parser::OpmlFeedEntry;
use crate::podcast::PodcastURL;

// This enum represents one "layer" of our command structure,
// including the 'next' command.
#[derive(Debug, Clone)]
pub enum PodcastCmd {
    EvalUrl(PodcastURL, Box<PodcastCmd>),
    Download(PodcastURL, Box<PodcastCmd>),
    Save(Box<PodcastCmd>),
    ProcessOpmlEntries(Vec<OpmlFeedEntry>, Box<PodcastCmd>),
    End,
}

impl PodcastCmd {
    pub fn eval_url(url: PodcastURL, next: PodcastCmd) -> Self {
        PodcastCmd::EvalUrl(url, Box::new(next))
    }

    // Helper to create EvalUrl from a string
    pub fn eval_url_from_str(url_str: &str, next: PodcastCmd) -> Self {
        PodcastCmd::EvalUrl(PodcastURL::new(url_str), Box::new(next))
    }

    pub fn download(url: PodcastURL, next: PodcastCmd) -> Self {
        PodcastCmd::Download(url, Box::new(next))
    }

    pub fn save(next: PodcastCmd) -> Self {
        PodcastCmd::Save(Box::new(next))
    }

    pub fn process_opml_entries(entries: Vec<OpmlFeedEntry>, next: PodcastCmd) -> Self {
        PodcastCmd::ProcessOpmlEntries(entries, Box::new(next))
    }

    pub fn end() -> Self {
        PodcastCmd::End
    }
}
