// src/player/player.rs
use anyhow::{anyhow, Result};
use log::{error, info, warn, trace};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::task;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::command::PlayerCommand;
use super::event::PlayerEvent;
use crate::podcast::Episode;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Error,
}

pub struct AudioPlayer {
    _output_stream: OutputStream,
    stream_handle: OutputStreamHandle,
    command_rx: mpsc::Receiver<PlayerCommand>,
    event_tx: broadcast::Sender<PlayerEvent>,
    active_sink: Arc<Mutex<Option<Sink>>>,
}

impl AudioPlayer {
    pub fn new(
        command_rx: mpsc::Receiver<PlayerCommand>,
        event_tx: broadcast::Sender<PlayerEvent>,
    ) -> Result<Self> {
        let (_output_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow!("Failed to create audio output stream: {}", e))?;
        Ok(Self {
            _output_stream,
            stream_handle,
            command_rx,
            event_tx,
            active_sink: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("AudioPlayer command loop started.");
        loop {
            // --- NEW DIAGNOSTIC LOG ---
            // This will tell us if the player task is getting a chance to run at all.
            trace!("Player task is alive, waiting for command...");

            // This is where the task will yield to the scheduler if no message is ready
            let command = match self.command_rx.recv().await {
                Some(cmd) => cmd,
                None => {
                    info!("Player command channel closed. Exiting player loop.");
                    break;
                }
            };

            if let Err(e) = self.handle_command(command).await {
                error!("[AudioPlayer] Error handling command: {}", e);
            }
        }
        Ok(())
    }

    async fn handle_command(&mut self, command: PlayerCommand) -> Result<()> {
        match command {
            PlayerCommand::PlayEpisode { episode } => {
                // --- THE LOG LINE YOU REQUESTED ---
                info!(
                    "Received PlayEpisode command for: '{}' with URL: {}",
                    episode.title(),
                    episode.audio_url()
                );

                self.stop_playback();

                let stream_handle = self.stream_handle.clone();
                let event_tx = self.event_tx.clone();
                let sink_handle = self.active_sink.clone();

                task::spawn_blocking(move || {
                    if let Err(e) = play_episode_on_thread(episode, stream_handle, event_tx, sink_handle) {
                        error!("Playback thread terminated with an error: {}", e);
                    }
                });
            }
            PlayerCommand::TogglePlayPause => self.toggle_play_pause()?,
            PlayerCommand::Stop => self.stop_playback(),
            _ => warn!("Command not yet implemented."),
        }
        Ok(())
    }

    fn stop_playback(&self) {
        if let Ok(mut sink_lock) = self.active_sink.lock() {
            if let Some(sink) = sink_lock.take() {
                info!("Stopping and dropping active sink.");
                sink.stop();
            }
        }
    }

    fn toggle_play_pause(&self) -> Result<()> {
        if let Some(sink) = self.active_sink.lock().unwrap().as_ref() {
            if sink.is_paused() {
                sink.play();
                info!("Playback resumed.");
                let _ = self.event_tx.send(PlayerEvent::Resumed);
            } else {
                sink.pause();
                info!("Playback paused.");
                let _ = self.event_tx.send(PlayerEvent::Paused);
            }
        } else {
            warn!("Toggle command received but no active sink.");
        }
        Ok(())
    }
}

fn play_episode_on_thread(
    episode: Episode,
    stream_handle: OutputStreamHandle,
    event_tx: broadcast::Sender<PlayerEvent>,
    sink_handle: Arc<Mutex<Option<Sink>>>,
) -> Result<()> {
    let result: Result<()> = (|| {
        info!("[Blocking Task] Executing for '{}'", episode.title());

        let _ = event_tx.send(PlayerEvent::Buffering);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
            
        let response = client.get(episode.audio_url()).send()?;

        if !response.status().is_success() {
            return Err(anyhow!("Download failed with status: {}", response.status()));
        }

        let audio_data = response.bytes()?.to_vec();
        info!("[Blocking Task] Download complete, size: {} bytes", audio_data.len());
        let cursor = Cursor::new(audio_data);

        let mss = MediaSourceStream::new(Box::new(symphonia::core::io::ReadOnlySource::new(cursor)), Default::default());

        let meta_opts: MetadataOptions = Default::default();
        let probed = symphonia::default::get_probe().format(&Hint::new(), mss, &FormatOptions::default(), &meta_opts)?;

        let reader = probed.format;
        let track = reader.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).cloned().ok_or_else(|| anyhow!("No supported audio track found"))?;
        let decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

        let sink = Sink::try_new(&stream_handle)?;
        let source = SymphoniaRodioSource::new(reader, decoder, track.id);
        sink.append(source);
        
        *sink_handle.lock().unwrap() = Some(sink);
        
        let podcast_title = "Podcast".to_string(); 
        let episode_title = episode.title().to_string();
        let _ = event_tx.send(PlayerEvent::Playing { podcast_title, episode_title });
        
        Ok(())
    })();

    if let Err(e) = &result {
        let _ = event_tx.send(PlayerEvent::Error(e.to_string()));
    }

    result
}

struct SymphoniaRodioSource {
    reader: Box<dyn FormatReader + Send>,
    decoder: Box<dyn Decoder + Send>,
    track_id: u32,
    buffer: Option<SampleBuffer<f32>>,
    pos: usize,
    channels: u16,
    sample_rate: u32,
}

impl SymphoniaRodioSource {
    fn new(reader: Box<dyn FormatReader + Send>, decoder: Box<dyn Decoder + Send>, track_id: u32) -> Self {
        let track = reader.tracks().iter().find(|t| t.id == track_id).expect("Track disappeared");
        let channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        Self { reader, decoder, track_id, buffer: None, pos: 0, channels, sample_rate }
    }
}

impl Iterator for SymphoniaRodioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(buffer) = &self.buffer {
                if self.pos < buffer.samples().len() {
                    let sample = buffer.samples()[self.pos];
                    self.pos += 1;
                    return Some(sample);
                }
            }
            
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(e) => {
                    trace!("Reader finished or failed: {}", e);
                    return None;
                }
            };

            if packet.track_id() != self.track_id { continue; }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut new_buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    new_buffer.copy_interleaved_ref(decoded);
                    self.buffer = Some(new_buffer);
                    self.pos = 0;
                }
                Err(e) => {
                    warn!("Decode error: {}", e);
                    continue;
                },
            }
        }
    }
}

impl Source for SymphoniaRodioSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { self.channels }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}