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
// use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions, FinalizeResult};
use symphonia::core::formats::{FormatOptions, FormatReader, Packet, SeekedTo, Track};
use symphonia::core::formats::{SeekMode, SeekTo};
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

struct MuteState {
    is_muted: bool,
    pre_mute_volume: f32,
}

impl Default for MuteState {
    fn default() -> Self {
        Self { is_muted: false, pre_mute_volume: 1.0 }
    }
}

struct PlaybackState {
    reader: Box<dyn FormatReader + Send>,
    decoder: Box<dyn Decoder + Send>,
    track_id: u32,
    track_duration: Duration,
}

pub struct AudioPlayer {
    _rodio_hardware_stream: OutputStream,
    rodio_hardware_stream_handle: OutputStreamHandle,
    player_command_receiver: mpsc::Receiver<PlayerRemoteCommand>,
    player_event_sender: broadcast::Sender<PlayerEvent>,
    active_rodio_sink: Arc<Mutex<Option<Sink>>>,
    active_playback_state: Arc<Mutex<Option<PlaybackState>>>,
    mute_state: Arc<Mutex<MuteState>>,
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
            active_playback_state: Arc::new(Mutex::new(None)),
            mute_state: Arc::new(Mutex::new(MuteState::default())),
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
                let active_playback_state: Arc<Mutex<Option<PlaybackState>>> =
                    self.active_playback_state.clone();

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
                        active_playback_state,
                    ) {
                        error!("Playback thread terminated with an error: {}", e);
                    }
                });
            }
            PlayerRemoteCommand::TogglePlayPause => self.toggle_play_pause()?,
            PlayerRemoteCommand::Stop => self.stop_playback(),
            PlayerRemoteCommand::SeekTo(position) => self.seek_to(position)?,
            PlayerRemoteCommand::VolumeUp(amount) => self.volume_up(amount)?,
            PlayerRemoteCommand::VolumeDown(amount) => self.volume_down(amount)?,
            PlayerRemoteCommand::ToggleMute => self.toggle_mute()?,
            // PlayerRemoteCommand::SkipForward(secs) => self.seek_forward(secs)?,
            // PlayerRemoteCommand::SkipBackward(secs) => self.seek_backward(secs)?,
            _ => warn!("Command {:?} not yet implemented.", player_command),
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
        // Clear the playback state as well
        *self.active_playback_state.lock().unwrap() = None;
    }

    fn toggle_play_pause(&self) -> Result<()> {
        let sink_lock: MutexGuard<Option<Sink>> =
            self.active_rodio_sink.lock().map_err(|e| anyhow!("Failed to lock sink: {}", e))?;

        // if let Some(sink) = sink_lock.as_ref() {
        //     if sink.is_paused() {
        //         sink.play();
        //         info!("Playback resumed.");
        //     }
        // }
        //
        // let sink_lock_attempt: LockResult<MutexGuard<Option<Sink>>> = self.active_rodio_sink.lock();
        // let sink_mutex: MutexGuard<Option<Sink>> =
        //     sink_lock_attempt.map_err(|e| anyhow!("Failed to lock sink: {}", e))?;

        if let Some(sink) = sink_lock.as_ref() {
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

    fn seek_to(&mut self, position: Duration) -> Result<()> {
        info!("Seeking to {:?}", position);
        // 1. Lock the playback state. If it doesn't exist, we can't seek.
        let mut playback_state_lock: MutexGuard<Option<PlaybackState>> =
            self.active_playback_state.lock().unwrap();
        let playback_state: &mut PlaybackState = match &mut *playback_state_lock {
            Some(playback_state) => playback_state,
            None => {
                warn!("Seek command received but no active playback state.");
                return Ok(());
            }
        };

        // 2. Stop the current sink to release the audio device.
        if let Some(sink) = self.active_rodio_sink.lock().unwrap().take() {
            sink.stop();
            info!("Playback stopped.");
            let _ = self.player_event_sender.send(PlayerEvent::Stopped);
        }

        // 3. Perform the seek on the Symphonia FormatReader.
        let seek_result: symphonia::core::errors::Result<SeekedTo> = playback_state.reader.seek(
            SeekMode::Accurate,
            SeekTo::Time { time: position.into(), track_id: Some(playback_state.track_id) },
        );

        if let Err(e) = seek_result {
            error!("Symphonia seek failed: {:?}", e);
            // Attempt to recover by just stopping.
            self.stop_playback();
            let _ =
                self.player_event_sender.send(PlayerEvent::Error(format!("Seek failed: {:?}", e)));
            return Err(anyhow!("Seek failed: {:?}", e));
        }

        // 4. Reset the decoder state.
        let _ = playback_state.decoder.reset();

        // 5. Create a new Sink and a new SymphoniaRodioSource.
        // The source will npw start pulling from the new position in the reader.
        let new_sink: Sink = Sink::try_new(&self.rodio_hardware_stream_handle)?;
        let new_source: SymphoniaRodioSource =
            SymphoniaRodioSource::from_playback_state(playback_state);
        new_sink.append(new_source);
        new_sink.play();

        *self.active_rodio_sink.lock().unwrap() = Some(new_sink);
        info!("Seek successful, Playback resumed from new position.");

        Ok(())
    }

    fn volume_up(&mut self, amount: f32) -> Result<()> {
        let sink_lock: MutexGuard<Option<Sink>> =
            self.active_rodio_sink.lock().map_err(|e| anyhow!("Failed to lock sink: {}", e))?;
        if let Some(sink) = sink_lock.as_ref() {
            let current_volume: f32 = sink.volume();
            let new_volume: f32 = (current_volume + amount).min(2.0); // Cap at 200%
            sink.set_volume(new_volume);
            info!("Volume increased to {:.2}", new_volume);

            let mut mute_state: MutexGuard<MuteState> =
                self.mute_state.lock().map_err(|e| anyhow!("Failed to lock mute state: {}", e))?;
            if new_volume > 0.0 {
                mute_state.is_muted = false;
            }

            let _ = self.player_event_sender.send(PlayerEvent::VolumeChanged(new_volume));
        }
        Ok(())
    }

    fn volume_down(&mut self, amount: f32) -> Result<()> {
        let sink_lock: MutexGuard<Option<Sink>> =
            self.active_rodio_sink.lock().map_err(|e| anyhow!("Failed to lock sink: {}", e))?;
        if let Some(sink) = sink_lock.as_ref() {
            let current_volume = sink.volume();
            let new_volume = (current_volume - amount).max(0.0);
            sink.set_volume(new_volume);
            info!("Volume decreased to {:.2}", new_volume);

            if new_volume == 0.0 {
                let mut mute_state: MutexGuard<MuteState> = self
                    .mute_state
                    .lock()
                    .map_err(|e| anyhow!("Failed to lock mute state: {}", e))?;
                if !mute_state.is_muted {
                    mute_state.is_muted = true;
                    mute_state.pre_mute_volume = current_volume;
                }
            }

            let _ = self.player_event_sender.send(PlayerEvent::VolumeChanged(new_volume));
        }
        Ok(())
    }
    fn toggle_mute(&mut self) -> Result<()> {
        let sink_lock: MutexGuard<Option<Sink>> =
            self.active_rodio_sink.lock().map_err(|e| anyhow!("Failed to lock sink: {}", e))?;
        if let Some(sink) = sink_lock.as_ref() {
            let mut mute_state: MutexGuard<MuteState> =
                self.mute_state.lock().map_err(|e| anyhow!("Failed to lock mute state: {}", e))?;

            if mute_state.is_muted {
                let new_volume: f32 = mute_state.pre_mute_volume;
                sink.set_volume(new_volume);
                mute_state.is_muted = false;
                info!("Unmuted. Volume restored to {:.2}", new_volume);
                let _ = self.player_event_sender.send(PlayerEvent::VolumeChanged(new_volume));
            } else {
                mute_state.pre_mute_volume = sink.volume();
                sink.set_volume(0.0);
                mute_state.is_muted = true;
                info!("Muted. Volume set to 0.0");
                let _ = self.player_event_sender.send(PlayerEvent::VolumeChanged(0.0));
            }
        }
        Ok(())
    }

    fn seek_forward(&self, secs: u64) -> Result<()> {
        let sink_lock: MutexGuard<Option<Sink>> = self
            .active_rodio_sink
            .lock()
            .map_err(|e| anyhow!("Failed to lock sink for seek: {}", e))?;

        if let Some(sink) = sink_lock.as_ref() {
            let current_pos: Duration = sink.get_pos();
            let seek_duration: Duration = Duration::from_secs(secs);
            let new_pos: Duration = current_pos + seek_duration;
            info!("Attempting to seek forward by {}s to {:?}", secs, new_pos);

            // Note: try_seek may fail if the underlying source is not "seekable" by rodio's
            // standards (which often requires the source to be Clone). If seeking doesn't
            // work, a more advanced source implementation may be required.
            if let Err(e) = sink.try_seek(new_pos) {
                warn!("Failed to seek forward: {:?}", e);
            }
        } else {
            warn!("Seek command received but no active sink.");
        }
        Ok(())
    }
    fn seek_backward(&self, secs: u64) -> Result<()> {
        let sink_lock: MutexGuard<Option<Sink>> = self
            .active_rodio_sink
            .lock()
            .map_err(|e| anyhow!("Failed to lock sink for seek: {}", e))?;

        if let Some(sink) = sink_lock.as_ref() {
            let current_pos: Duration = sink.get_pos();
            let seek_duration: Duration = Duration::from_secs(secs);
            let new_pos: Duration = current_pos.saturating_sub(seek_duration);
            info!("Attempting to seek backward by {}s to {:?}", secs, new_pos);

            if let Err(e) = sink.try_seek(new_pos) {
                warn!("Failed to seek backward: {:?}", e);
            }
        } else {
            warn!("Seek command received but no active sink.");
        }
        Ok(())
    }
}

fn play_episode_on_thread(
    episode: Episode,
    rodio_hardware_stream_handle: OutputStreamHandle,
    player_event_sender: broadcast::Sender<PlayerEvent>,
    active_rodio_sink: Arc<Mutex<Option<Sink>>>,
    active_playback_state: Arc<Mutex<Option<PlaybackState>>>,
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

        let mut hint: Hint = Hint::new();
        // Provide a hint to Symphonia about the container format if possible
        if episode.audio_url().ends_with(".mp3") {
            hint.with_extension("mp3");
        }

        let probed: ProbeResult = symphonia::default::get_probe().format(
            &hint,
            mss,
            &FormatOptions { enable_gapless: true, ..Default::default() },
            &MetadataOptions::default(),
        )?;

        // let audio_format_reader: Box<dyn FormatReader> = probed.format;
        let audio_format_reader: Box<dyn FormatReader> = probed.format;
        let track: Track = audio_format_reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .cloned()
            .ok_or_else(|| anyhow!("No supported audio track found"))?;
        let decoder: Box<dyn Decoder> = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;

        // // Audio File → FormatReader → Decoder → SymphoniaRodioSource → Sink → Hardware
        // //     ↑            ↑           ↑              ↑                 ↑        ↑
        // //   Raw bytes   Packets    Samples      Rodio format         Playback Speakers
        // let rodio_sink: Sink = Sink::try_new(&rodio_hardware_stream_handle)?;
        // let symphonia_source: SymphoniaRodioSource =
        //     SymphoniaRodioSource::new(audio_format_reader, decoder, track.id);
        // rodio_sink.append(symphonia_source);
        //
        // *active_rodio_sink.lock().unwrap() = Some(rodio_sink);

        let track_id: u32 = track.id;
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

        // Create the state needed for playback and seeking
        let mut state = PlaybackState {
            reader: audio_format_reader,
            decoder,
            track_id,
            track_duration: duration,
        };

        let rodio_source = SymphoniaRodioSource::from_playback_state(&mut state);
        let rodio_sink: Sink = Sink::try_new(&rodio_hardware_stream_handle)?;

        rodio_sink.append(rodio_source);
        rodio_sink.play();

        *active_rodio_sink.lock().unwrap() = Some(rodio_sink);
        *active_playback_state.lock().unwrap() = Some(state);

        // Send the initial playback event
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
                    } else if sink.empty() {
                        info!("Sink is empty, progress loop will exit.");
                        break;
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
    // fn new(
    //     reader: Box<dyn FormatReader + Send>,
    //     decoder: Box<dyn Decoder + Send>,
    //     track_id: u32,
    fn from_playback_state(state: &mut PlaybackState) -> Self {
        // let track: &Track =
        //     reader.tracks().iter().find(|t| t.id == track_id).expect("Track disappeared");
        let track = state
            .reader
            .tracks()
            .iter()
            .find(|t| t.id == state.track_id)
            .expect("Track disappeared");
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
