// src/player/player.rs
use anyhow::{Result, anyhow};
use log::{error, info, trace, warn};
use reqwest::blocking::{Client, Response};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::io::{Cursor, sink};
use std::sync::{Arc, LockResult, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::task;

use super::command::PlayerRemoteCommand;
use super::event::PlayerEvent;
use crate::podcast::Episode;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, Packet, Track};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::{Hint, ProbeResult};
use symphonia::core::units::Time;
use tokio::sync::broadcast::Sender;
use tokio::time::Interval;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Error,
}

pub struct AudioPlayer {
    _rodio_hardware_stream: OutputStream,
    rodio_hardware_stream_handle: OutputStreamHandle,
    player_command_receiver: mpsc::Receiver<PlayerRemoteCommand>,
    player_event_sender: broadcast::Sender<PlayerEvent>,
    active_rodio_sink: Arc<Mutex<Option<Sink>>>,
}

impl AudioPlayer {
    pub fn new(
        player_command_receiver: mpsc::Receiver<PlayerRemoteCommand>,
        player_event_sender: broadcast::Sender<PlayerEvent>,
    ) -> Result<Self> {
        let (_rodio_hardware_stream, rodio_hardware_stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow!("Failed to create audio output stream: {}", e))?;
        Ok(Self {
            _rodio_hardware_stream,
            rodio_hardware_stream_handle,
            player_command_receiver,
            player_event_sender,
            active_rodio_sink: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // This async task's only job is to wait efficiently for commands on the player_command_receiver.
        // It's very cheap and doesn't occupy an OS thread while it's waiting.
        info!("AudioPlayer command loop started.");
        loop {
            // This will tell us if the player task is getting a chance to run at all.
            trace!("Player task is alive, waiting for command...");

            // This `await` point is where the task yields control back to the
            // Tokio scheduler. It doesn't consume CPU while waiting.
            // This is where the task will yield to the scheduler if no message is ready
            let player_command: PlayerRemoteCommand =
                match self.player_command_receiver.recv().await {
                    Some(cmd) => cmd,
                    None => {
                        info!("Player command channel closed. Exiting player loop.");
                        break;
                    }
                };

            if let Err(e) = self.dispatch_player_command(player_command).await {
                error!("[AudioPlayer] Error handling command: {}", e);
            }
        }
        Ok(())
    }

    async fn dispatch_player_command(&mut self, player_command: PlayerRemoteCommand) -> Result<()> {
        match player_command {
            // A PlayEpisode command is sent by the App.
            // The AudioPlayer's lightweight async task wakes up on a Tokio worker thread.
            // It immediately spawns the heavy work onto a separate, blocking-safe OS thread.
            // This blocking thread downloads, decodes, and hands the audio off to rodio.
            // The rodio library plays the audio using its own dedicated audio thread.
            PlayerRemoteCommand::PlayEpisode { episode } => {
                info!(
                    "Received PlayEpisode command for: {} duration {} with URL: {}",
                    episode.title(),
                    episode.duration().unwrap_or("unknown"),
                    episode.audio_url()
                );

                self.stop_playback();

                let rodio_stream_handle: OutputStreamHandle =
                    self.rodio_hardware_stream_handle.clone();
                let player_event_sender: Sender<PlayerEvent> = self.player_event_sender.clone();
                let active_rodio_sink: Arc<Mutex<Option<Sink>>> = self.active_rodio_sink.clone();

                // This work is going to block. Please run it on a background thread from
                // your dedicated blocking-thread-pool so it doesn't interfere with my
                // main async tasks.
                task::spawn_blocking(move || {
                    // All code inside this closure runs on a different thread pool managed by Tokio,
                    // one designed for blocking operations.
                    if let Err(e) = play_episode_on_thread(
                        episode,
                        rodio_stream_handle,
                        player_event_sender,
                        active_rodio_sink,
                    ) {
                        error!("Playback thread terminated with an error: {}", e);
                    }
                });
            }
            PlayerRemoteCommand::TogglePlayPause => self.toggle_play_pause()?,
            PlayerRemoteCommand::Stop => self.stop_playback(),
            _ => warn!("Command not yet implemented."),
        }
        Ok(())
    }

    fn stop_playback(&self) {
        if let Ok(mut sink_lock) = self.active_rodio_sink.lock() {
            if let Some(sink) = sink_lock.take() {
                sink.stop();
                info!("Playback stopped.");
                let _ = self.player_event_sender.send(PlayerEvent::Stopped);
            }
        }
    }

    fn toggle_play_pause(&self) -> Result<()> {
        let sink_lock_attempt: LockResult<MutexGuard<Option<Sink>>> = self.active_rodio_sink.lock();
        let sink_mutex: MutexGuard<Option<Sink>> =
            sink_lock_attempt.map_err(|e| anyhow!("Failed to lock sink: {}", e))?;

        if let Some(sink) = sink_mutex.as_ref() {
            if sink.is_paused() {
                sink.play();
                info!("Playback resumed.");
                let _ = self.player_event_sender.send(PlayerEvent::Resumed);
            } else {
                sink.pause();
                info!("Playback paused.");
                let _ = self.player_event_sender.send(PlayerEvent::Paused);
            }
        } else {
            warn!("Toggle command received but no active sink.");
        }
        Ok(())
    }
}

fn play_episode_on_thread(
    episode: Episode,
    rodio_hardware_stream_handle: OutputStreamHandle,
    player_event_sender: broadcast::Sender<PlayerEvent>,
    active_rodio_sink: Arc<Mutex<Option<Sink>>>,
) -> Result<()> {
    let result: Result<()> = (|| {
        info!("[Blocking Task] Executing for '{}'", episode.title());

        let _ = player_event_sender.send(PlayerEvent::Buffering);

        // A blocking network request.
        let http_client: Client =
            reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).build()?;

        // Consider a **progressive download** approach:
        // - Start playback after downloading first N seconds
        // - Continue downloading in background
        let response: Response = http_client.get(episode.audio_url()).send()?;

        if !response.status().is_success() {
            return Err(anyhow!("Download failed with status: {}", response.status()));
        }

        // Waits for the entire file to download.
        let audio_data: Vec<u8> = response.bytes()?.to_vec();
        info!("[Blocking Task] Download complete, size: {} bytes", audio_data.len());

        let audio_data_with_cursor: Cursor<Vec<u8>> = Cursor::new(audio_data);
        let audio_source: ReadOnlySource<Cursor<Vec<u8>>> =
            symphonia::core::io::ReadOnlySource::new(audio_data_with_cursor);
        let mss: MediaSourceStream =
            MediaSourceStream::new(Box::new(audio_source), Default::default());

        let MetadataOptions { limit_metadata_bytes, limit_visual_bytes }: MetadataOptions =
            Default::default();
        let probed: ProbeResult = symphonia::default::get_probe().format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions { limit_metadata_bytes, limit_visual_bytes },
        )?;

        let audio_format_reader: Box<dyn FormatReader> = probed.format;
        let track: Track = audio_format_reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .cloned()
            .ok_or_else(|| anyhow!("No supported audio track found"))?;
        let decoder: Box<dyn Decoder> = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;

        // Audio File → FormatReader → Decoder → SymphoniaRodioSource → Sink → Hardware
        //     ↑            ↑           ↑              ↑                 ↑        ↑
        //   Raw bytes   Packets    Samples      Rodio format         Playback Speakers
        let rodio_sink: Sink = Sink::try_new(&rodio_hardware_stream_handle)?;
        let symphonia_source: SymphoniaRodioSource =
            SymphoniaRodioSource::new(audio_format_reader, decoder, track.id);
        rodio_sink.append(symphonia_source);

        *active_rodio_sink.lock().unwrap() = Some(rodio_sink);

        // Extract duration from the track
        let duration: Duration = track
            .codec_params
            .time_base
            .and_then(|time_base| {
                track.codec_params.n_frames.map(|frames| {
                    let symphonia_time: Time = time_base.calc_time(frames);
                    Duration::from_secs_f64(symphonia_time.seconds as f64 + symphonia_time.frac)
                })
            })
            .unwrap_or(Duration::ZERO);

        let podcast_title: String = episode.podcast_name().to_string();
        let episode_title: String = episode.title().to_string();
        let _ = player_event_sender.send(PlayerEvent::Playing {
            podcast_title,
            episode_title,
            duration,
        });

        let sink_clone: Arc<Mutex<Option<Sink>>> = active_rodio_sink.clone();
        let sender_clone: Sender<PlayerEvent> = player_event_sender.clone();

        tokio::spawn(async move {
            let ms_500: Duration = tokio::time::Duration::from_millis(500);
            let mut interval: Interval = tokio::time::interval(ms_500);
            loop {
                interval.tick().await;
                if let Some(sink) = sink_clone.lock().unwrap().as_ref() {
                    if !sink.is_paused() && !sink.empty() {
                        let elapsed: Duration = sink.get_pos();
                        let _ = sender_clone.send(PlayerEvent::Progress {
                            current_position: elapsed,
                            total_duration: duration,
                        });
                    }
                } else {
                    break;
                }
            }
        });

        Ok(())
    })();

    if let Err(e) = &result {
        let _ = player_event_sender.send(PlayerEvent::Error(e.to_string()));
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
    fn new(
        reader: Box<dyn FormatReader + Send>,
        decoder: Box<dyn Decoder + Send>,
        track_id: u32,
    ) -> Self {
        let track: &Track =
            reader.tracks().iter().find(|t| t.id == track_id).expect("Track disappeared");
        let channels: u16 = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
        let sample_rate: u32 = track.codec_params.sample_rate.unwrap_or(44100);
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

            let packet: Packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(e) => {
                    trace!("Reader finished or failed: {}", e);
                    return None;
                }
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut new_buffer: SampleBuffer<f32> =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    new_buffer.copy_interleaved_ref(decoded);
                    self.buffer = Some(new_buffer);
                    self.pos = 0;
                }
                Err(e) => {
                    warn!("Decode error: {}", e);
                    continue;
                }
            }
        }
    }
}

impl Source for SymphoniaRodioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
