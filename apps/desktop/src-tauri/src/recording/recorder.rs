//! Runs a recording: drives the captures, the compositor, the mixer and the
//! encoder, and finalizes the file.
//!
//! ```text
//! ScreenCapture ──▶ latest screen frame ─┐
//! CameraCapture ──▶ latest camera frame ─┼─▶ video thread (fixed fps) ─┐
//!                                        │                              ├─▶ VideoEncoder ─▶ MP4
//! MicrophoneCapture ─┐                   │                              │
//! SystemAudioCapture ┼─▶ AudioMixer ◀─ music                            │
//!                    └──────────────────▶ audio thread (every 20 ms) ───┘
//! ```
//!
//! A supervisor thread owns the capture objects. It handles pause, resume and
//! stop commands, publishes status updates, and stops the recording on its
//! own — still finalizing the file — when a capture or the encoder fails or
//! the disk is nearly full.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Local;
use parking_lot::Mutex;
use serde::Serialize;

use super::capture::{
    AudioSink, CameraCapture, EncoderSettings, MicrophoneCapture, ScreenCapture,
    ScreenCaptureRequest, SharedFrame, SystemAudioCapture, VideoEncoder, VideoSink,
};
use super::clock::MediaClock;
use super::session::{RecordingSession, SessionSummary};
use super::state::{RecordingEvent, RecordingState};
use crate::audio::mixer::{AudioMixer, MixerConfig, MusicTrack, SourceKind};
use crate::config::{output_size, video_bitrate, RecordingConfig};
use crate::error::{AppError, AppResult};
use crate::output::{self, OutputPlan};
use crate::platform::Platform;
use crate::video::compositor::{self, BgraMut, Mask, Rect};

/// How far behind real time the mixer runs, giving late audio a chance to arrive.
const AUDIO_LATENCY: f64 = 0.2;
const AUDIO_TICK: Duration = Duration::from_millis(20);
const STATUS_TICK: Duration = Duration::from_millis(250);
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
/// A device that has been silent this long is reported as a warning.
const STALE_DEVICE_SECONDS: f64 = 3.0;
const DISK_CHECK_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub state: RecordingState,
    pub elapsed_ms: u64,
    pub microphone_level: f32,
    pub system_audio_level: f32,
    pub warnings: Vec<String>,
}

/// The result of a recording, reported once it has been finalized.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOutcome {
    /// Where the video was saved, if any media was written.
    pub path: Option<PathBuf>,
    pub duration_ms: u64,
    /// False if the file could not be finalized cleanly.
    pub complete: bool,
    /// Why the recording ended early or could not be finalized.
    pub error: Option<AppError>,
    pub warnings: Vec<String>,
}

/// Receives recorder notifications (implemented by the Tauri layer).
pub trait RecorderEvents: Send + Sync + 'static {
    fn status(&self, status: &RecordingStatus);
    fn finished(&self, outcome: &RecordingOutcome);
}

/// State shared between the capture callbacks and the recorder threads.
struct Shared {
    clock: Mutex<Option<MediaClock>>,
    state: Mutex<RecordingState>,
    screen: Mutex<Option<SharedFrame>>,
    camera: Mutex<Option<(SharedFrame, f64)>>,
    mixer: Mutex<AudioMixer>,
    encoder: Mutex<Box<dyn VideoEncoder>>,
    running: AtomicBool,
    /// Media time of the last encoded video frame.
    last_video_time: Mutex<Option<f64>>,
    /// First fatal error; setting it makes the supervisor stop the recording.
    failure: Mutex<Option<AppError>>,
    warnings: Mutex<Vec<String>>,
    last_microphone: Mutex<Option<f64>>,
    last_system_audio: Mutex<Option<f64>>,
}

impl Shared {
    fn fail(&self, error: AppError) {
        log::error!("recording failed: {error}");
        let mut failure = self.failure.lock();
        if failure.is_none() {
            *failure = Some(error);
        }
    }

    fn warn(&self, message: impl Into<String>) {
        let message = message.into();
        let mut warnings = self.warnings.lock();
        if !warnings.contains(&message) {
            log::warn!("{message}");
            warnings.push(message);
        }
    }

    fn media_time(&self, host_time: f64) -> Option<f64> {
        self.clock.lock().as_ref()?.media_time(host_time)
    }

    fn transition(&self, event: RecordingEvent) -> AppResult<RecordingState> {
        let mut state = self.state.lock();
        *state = state.next(event)?;
        Ok(*state)
    }
}

struct ScreenSink(Arc<Shared>);

impl VideoSink for ScreenSink {
    fn frame(&self, frame: SharedFrame, _host_time: f64) {
        *self.0.screen.lock() = Some(frame);
    }

    fn error(&self, error: AppError) {
        self.0.fail(error);
    }
}

struct CameraSink(Arc<Shared>, fn() -> f64);

impl VideoSink for CameraSink {
    fn frame(&self, frame: SharedFrame, _host_time: f64) {
        *self.0.camera.lock() = Some((frame, (self.1)()));
    }

    fn error(&self, error: AppError) {
        self.0.warn(format!("Camera stopped: {error}"));
    }
}

struct AudioSourceSink {
    shared: Arc<Shared>,
    kind: SourceKind,
    host_time: fn() -> f64,
}

impl AudioSink for AudioSourceSink {
    fn audio(&self, host_time: f64, samples: &[f32]) {
        let last = match self.kind {
            SourceKind::Microphone => &self.shared.last_microphone,
            SourceKind::SystemAudio => &self.shared.last_system_audio,
        };
        *last.lock() = Some((self.host_time)());
        if let Some(media_time) = self.shared.media_time(host_time) {
            self.shared
                .mixer
                .lock()
                .push(self.kind, media_time, samples);
        }
    }

    fn error(&self, error: AppError) {
        let name = match self.kind {
            SourceKind::Microphone => "Microphone",
            SourceKind::SystemAudio => "System audio",
        };
        self.shared.warn(format!("{name} stopped: {error}"));
    }
}

/// The capture objects of a running recording.
#[derive(Default)]
struct Captures {
    screen: Option<Box<dyn ScreenCapture>>,
    camera: Option<Box<dyn CameraCapture>>,
    microphone: Option<Box<dyn MicrophoneCapture>>,
    system_audio: Option<Box<dyn SystemAudioCapture>>,
}

impl Captures {
    fn stop(&mut self) {
        if let Some(mut c) = self.screen.take() {
            c.stop();
        }
        if let Some(mut c) = self.camera.take() {
            c.stop();
        }
        if let Some(mut c) = self.microphone.take() {
            c.stop();
        }
        if let Some(mut c) = self.system_audio.take() {
            c.stop();
        }
    }
}

enum Command {
    Pause(mpsc::Sender<AppResult<()>>),
    Resume(mpsc::Sender<AppResult<()>>),
    Stop(mpsc::Sender<RecordingOutcome>),
}

/// A recording in progress. Dropping it without calling [`stop`] still stops
/// and finalizes the recording in the background.
///
/// [`stop`]: RecordingHandle::stop
pub struct RecordingHandle {
    commands: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    session: RecordingSession,
}

impl RecordingHandle {
    pub fn summary(&self) -> SessionSummary {
        self.session.summary()
    }

    pub fn state(&self) -> RecordingState {
        *self.shared.state.lock()
    }

    pub fn pause(&self) -> AppResult<()> {
        self.request(Command::Pause)
    }

    pub fn resume(&self) -> AppResult<()> {
        self.request(Command::Resume)
    }

    fn request(&self, command: fn(mpsc::Sender<AppResult<()>>) -> Command) -> AppResult<()> {
        let (tx, rx) = mpsc::channel();
        self.commands
            .send(command(tx))
            .map_err(|_| AppError::InvalidState("the recording has already ended".into()))?;
        rx.recv()
            .map_err(|_| AppError::InvalidState("the recording has already ended".into()))?
    }

    /// Stops and finalizes the recording, blocking until the file is written.
    pub fn stop(self) -> AppResult<RecordingOutcome> {
        let (tx, rx) = mpsc::channel();
        self.commands
            .send(Command::Stop(tx))
            .map_err(|_| AppError::InvalidState("the recording has already ended".into()))?;
        rx.recv()
            .map_err(|_| AppError::InvalidState("the recording has already ended".into()))
    }

    /// Whether the supervisor has finished (e.g. after an automatic stop).
    pub fn is_finished(&self) -> bool {
        *self.shared.state.lock() == RecordingState::Idle
    }
}

/// Everything needed to start a recording, resolved by the caller.
pub struct StartRequest {
    pub config: RecordingConfig,
    /// Path of the selected background music file, if any.
    pub music_path: Option<PathBuf>,
}

pub fn start<P: Platform>(
    request: StartRequest,
    events: Arc<dyn RecorderEvents>,
) -> AppResult<RecordingHandle> {
    let StartRequest { config, music_path } = request;
    config.validate()?;
    let capabilities = P::capabilities();
    if !capabilities.recording.supported {
        return Err(AppError::Unsupported(
            capabilities.recording.reason.unwrap_or_default(),
        ));
    }
    if config.system_audio && !capabilities.system_audio.supported {
        return Err(AppError::Unsupported(
            capabilities
                .system_audio
                .reason
                .unwrap_or_else(|| "System audio capture is not supported on this system.".into()),
        ));
    }

    let source_size = P::source_size(&config.source)?;
    let size = output_size(source_size, config.resolution);
    let started_at = Local::now();
    let plan = output::plan_output(
        &config.output_directory,
        &started_at,
        P::INTERMEDIATE_EXTENSION,
        |p| p.exists(),
    );
    output::prepare_output(&plan, output::available_space(&config.output_directory))?;

    let music = match (&config.music, &music_path) {
        (Some(music), Some(path)) => Some(MusicTrack {
            samples: Arc::new(P::decode_audio_file(path)?),
            gain: music.volume,
        }),
        (Some(music), None) => {
            return Err(AppError::Music(format!(
                "unknown track \"{}\"",
                music.track_id
            )))
        }
        _ => None,
    };

    let session = RecordingSession {
        source_size,
        output_size: size,
        video_bitrate: video_bitrate(size, config.fps),
        started_at,
        output: plan,
        config,
    };

    let encoder = P::encoder(&EncoderSettings {
        path: session.output.partial_path.clone(),
        size,
        fps: session.config.fps,
        video_bitrate: session.video_bitrate,
        audio: session.config.has_audio(),
    })
    .inspect_err(|_| output::discard(&session.output))?;

    let shared = Arc::new(Shared {
        clock: Mutex::new(None),
        state: Mutex::new(RecordingState::Idle.next(RecordingEvent::Start)?),
        screen: Mutex::new(None),
        camera: Mutex::new(None),
        mixer: Mutex::new(AudioMixer::new(MixerConfig {
            microphone_gain: session.config.microphone.as_ref().map(|_| 1.0),
            system_audio_gain: session.config.system_audio.then_some(1.0),
            music,
        })),
        encoder: Mutex::new(encoder),
        running: AtomicBool::new(true),
        last_video_time: Mutex::new(None),
        failure: Mutex::new(None),
        warnings: Mutex::new(Vec::new()),
        last_microphone: Mutex::new(None),
        last_system_audio: Mutex::new(None),
    });

    let mut captures = Captures::default();
    if let Err(error) = start_captures::<P>(&session, &shared, &mut captures) {
        captures.stop();
        drop(shared);
        output::discard(&session.output);
        return Err(error);
    }

    let host_time = P::host_time;
    *shared.clock.lock() = Some(MediaClock::new(host_time()));
    shared.transition(RecordingEvent::Started)?;

    let threads = spawn_media_threads(&session, &shared, host_time);
    let (commands, receiver) = mpsc::channel();
    let supervisor = Supervisor {
        session: session.clone(),
        shared: shared.clone(),
        captures,
        threads: Some(threads),
        events,
        host_time,
        finalize_recording: P::finalize_recording,
    };
    thread::Builder::new()
        .name("codetake-recorder".into())
        .spawn(move || supervisor.run(receiver))
        .map_err(|e| AppError::InvalidState(format!("cannot start recorder thread: {e}")))?;

    Ok(RecordingHandle {
        commands,
        shared,
        session,
    })
}

fn start_captures<P: Platform>(
    session: &RecordingSession,
    shared: &Arc<Shared>,
    captures: &mut Captures,
) -> AppResult<()> {
    let config = &session.config;

    let mut screen = P::screen_capture(ScreenCaptureRequest {
        source: config.source.clone(),
        output_size: session.output_size,
        fps: config.fps.as_u32(),
        show_cursor: true,
    })?;
    screen.start(Arc::new(ScreenSink(shared.clone())))?;
    captures.screen = Some(screen);

    if let Some(camera_config) = &config.camera {
        let mut camera = P::camera_capture(&camera_config.device_id)?;
        camera.start(Arc::new(CameraSink(shared.clone(), P::host_time)))?;
        captures.camera = Some(camera);
    }
    if let Some(mic_config) = &config.microphone {
        let mut microphone = P::microphone_capture(&mic_config.device_id)?;
        microphone.start(Arc::new(AudioSourceSink {
            shared: shared.clone(),
            kind: SourceKind::Microphone,
            host_time: P::host_time,
        }))?;
        captures.microphone = Some(microphone);
    }
    if config.system_audio {
        let mut system_audio = P::system_audio_capture()?;
        system_audio.start(Arc::new(AudioSourceSink {
            shared: shared.clone(),
            kind: SourceKind::SystemAudio,
            host_time: P::host_time,
        }))?;
        captures.system_audio = Some(system_audio);
    }

    // Don't start the clock until the screen is actually delivering frames,
    // so the video never begins with a black gap.
    let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
    while shared.screen.lock().is_none() {
        if let Some(error) = shared.failure.lock().take() {
            return Err(error);
        }
        if Instant::now() > deadline {
            return Err(AppError::ScreenCapture(
                "no frames were received from the screen".into(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

struct MediaThreads {
    video: JoinHandle<()>,
    audio: Option<JoinHandle<()>>,
}

fn spawn_media_threads(
    session: &RecordingSession,
    shared: &Arc<Shared>,
    host_time: fn() -> f64,
) -> MediaThreads {
    let overlay = session.config.camera.as_ref().map(|camera| {
        let rect = compositor::overlay_rect(
            session.output_size.width as usize,
            session.output_size.height as usize,
            &camera.overlay,
        );
        (
            rect,
            Mask::new(rect.width, rect.height, camera.overlay.shape),
        )
    });
    let fps = session.config.fps.as_u32();

    let video = {
        let shared = shared.clone();
        thread::Builder::new()
            .name("codetake-video".into())
            .spawn(move || video_loop(&shared, fps, overlay, host_time))
            .expect("spawning a thread only fails when the OS is out of resources")
    };
    let audio = session.config.has_audio().then(|| {
        let shared = shared.clone();
        thread::Builder::new()
            .name("codetake-audio".into())
            .spawn(move || audio_loop(&shared, host_time))
            .expect("spawning a thread only fails when the OS is out of resources")
    });
    MediaThreads { video, audio }
}

fn sleep_until(host_time: fn() -> f64, target: f64) {
    let delay = target - host_time();
    if delay > 0.0 {
        thread::sleep(Duration::from_secs_f64(delay));
    }
}

/// Composes and encodes frames at a constant rate using the most recent
/// screen and camera frames. Running at a fixed rate (rather than on screen
/// updates) keeps the webcam moving even when the screen is static.
fn video_loop(shared: &Shared, fps: u32, overlay: Option<(Rect, Mask)>, host_time: fn() -> f64) {
    let interval = 1.0 / f64::from(fps);
    let mut next_tick = host_time();
    let mut last_media_time = f64::NEG_INFINITY;

    while shared.running.load(Ordering::Acquire) {
        sleep_until(host_time, next_tick);
        next_tick += interval;
        let now = host_time();
        if now - next_tick > 0.25 {
            // We fell far behind (e.g. the machine was busy); don't burst.
            next_tick = now + interval;
        }

        let Some(media_time) = shared.media_time(now) else {
            continue; // paused
        };
        if media_time <= last_media_time {
            continue;
        }
        let Some(screen) = shared.screen.lock().clone() else {
            continue;
        };
        let camera = shared
            .camera
            .lock()
            .as_ref()
            .map(|(frame, _)| frame.clone());

        let mut draw = |dst: &mut BgraMut<'_>| {
            screen.read(&mut |src| {
                if src.width == dst.width && src.height == dst.height {
                    compositor::copy_frame(src, dst);
                } else {
                    compositor::clear_frame(dst);
                    compositor::scale_frame_nearest(src, dst);
                }
            });
            if let (Some(camera), Some((rect, mask))) = (&camera, &overlay) {
                camera.read(&mut |src| compositor::blend_overlay(dst, src, *rect, mask, true));
            }
        };

        let result = shared.encoder.lock().append_video(media_time, &mut draw);
        match result {
            Ok(true) => {
                last_media_time = media_time;
                *shared.last_video_time.lock() = Some(media_time);
            }
            Ok(false) => log::debug!("encoder busy, dropped a frame at {media_time:.3}s"),
            Err(error) => {
                shared.fail(error);
                break;
            }
        }
    }
}

fn mix_and_encode(shared: &Shared, until: f64) -> AppResult<()> {
    let block = shared.mixer.lock().mix_until(until);
    if let Some(block) = block {
        shared
            .encoder
            .lock()
            .append_audio(block.start_time(), &block.samples)?;
    }
    Ok(())
}

fn audio_loop(shared: &Shared, host_time: fn() -> f64) {
    while shared.running.load(Ordering::Acquire) {
        thread::sleep(AUDIO_TICK);
        let elapsed = match shared.clock.lock().as_ref() {
            Some(clock) => clock.elapsed(host_time()),
            None => continue,
        };
        if let Err(error) = mix_and_encode(shared, elapsed - AUDIO_LATENCY) {
            shared.fail(error);
            break;
        }
    }
}

struct Supervisor {
    session: RecordingSession,
    shared: Arc<Shared>,
    captures: Captures,
    threads: Option<MediaThreads>,
    events: Arc<dyn RecorderEvents>,
    host_time: fn() -> f64,
    finalize_recording: fn(&std::path::Path, &std::path::Path) -> AppResult<()>,
}

impl Supervisor {
    fn run(mut self, commands: mpsc::Receiver<Command>) {
        let mut last_disk_check = Instant::now();
        loop {
            match commands.recv_timeout(STATUS_TICK) {
                Ok(Command::Pause(reply)) => {
                    let _ = reply.send(self.pause());
                }
                Ok(Command::Resume(reply)) => {
                    let _ = reply.send(self.resume());
                }
                Ok(Command::Stop(reply)) => {
                    let outcome = self.finish(None);
                    self.events.finished(&outcome);
                    let _ = reply.send(outcome);
                    return;
                }
                // The handle was dropped: finalize so nothing is lost.
                Err(RecvTimeoutError::Disconnected) => {
                    let outcome = self.finish(None);
                    self.events.finished(&outcome);
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }

            if last_disk_check.elapsed() >= DISK_CHECK_INTERVAL {
                last_disk_check = Instant::now();
                self.check_disk_space();
            }
            self.check_stale_devices();

            let failure = self.shared.failure.lock().take();
            if let Some(error) = failure {
                let outcome = self.finish(Some(error));
                self.events.finished(&outcome);
                return;
            }
            self.events.status(&self.status());
        }
    }

    fn pause(&self) -> AppResult<()> {
        self.shared.transition(RecordingEvent::Pause)?;
        if let Some(clock) = self.shared.clock.lock().as_mut() {
            clock.pause((self.host_time)());
        }
        self.events.status(&self.status());
        Ok(())
    }

    fn resume(&self) -> AppResult<()> {
        self.shared.transition(RecordingEvent::Resume)?;
        if let Some(clock) = self.shared.clock.lock().as_mut() {
            clock.resume((self.host_time)());
        }
        self.events.status(&self.status());
        Ok(())
    }

    fn elapsed(&self) -> f64 {
        self.shared
            .clock
            .lock()
            .as_ref()
            .map_or(0.0, |clock| clock.elapsed((self.host_time)()))
    }

    fn status(&self) -> RecordingStatus {
        let levels = self.shared.mixer.lock().take_levels();
        RecordingStatus {
            state: *self.shared.state.lock(),
            elapsed_ms: (self.elapsed() * 1000.0) as u64,
            microphone_level: levels.microphone,
            system_audio_level: levels.system_audio,
            warnings: self.shared.warnings.lock().clone(),
        }
    }

    fn check_disk_space(&self) {
        let free = output::available_space(&self.session.output.partial_path);
        if free.is_some_and(|free| free < output::MIN_FREE_BYTES_WHILE_RECORDING) {
            self.shared.fail(AppError::Storage(
                "the disk is almost full, so the recording was stopped and saved".into(),
            ));
        }
    }

    fn check_stale_devices(&self) {
        if *self.shared.state.lock() != RecordingState::Recording {
            return;
        }
        let now = (self.host_time)();
        let stale = |last: Option<f64>| last.is_some_and(|t| now - t > STALE_DEVICE_SECONDS);
        if stale(self.shared.camera.lock().as_ref().map(|(_, t)| *t)) {
            self.shared
                .warn("The camera stopped sending video. It may have been disconnected.");
        }
        if stale(*self.shared.last_microphone.lock()) {
            self.shared
                .warn("The microphone stopped sending audio. It may have been disconnected.");
        }
    }

    /// Stops everything and finalizes the file. Never loses written media:
    /// if the file cannot be finalized it is kept as `-incomplete.mp4`.
    fn finish(&mut self, error: Option<AppError>) -> RecordingOutcome {
        let _ = self.shared.transition(if error.is_some() {
            RecordingEvent::Fail
        } else {
            RecordingEvent::Stop
        });
        let stop_time = self.elapsed();

        self.shared.running.store(false, Ordering::Release);
        if let Some(threads) = self.threads.take() {
            let _ = threads.video.join();
            if let Some(audio) = threads.audio {
                let _ = audio.join();
            }
        }
        // End the session after the last frame's full duration, so the final
        // frame is shown for its whole duration; audio is flushed up to the
        // same point.
        let frame_duration = 1.0 / f64::from(self.session.config.fps.as_u32());
        let end_time = self
            .shared
            .last_video_time
            .lock()
            .map_or(stop_time, |last| stop_time.max(last + frame_duration));
        self.captures.stop();

        let written = (|| -> AppResult<()> {
            if self.session.config.has_audio() {
                mix_and_encode(&self.shared, end_time)?;
            }
            self.shared.encoder.lock().finish(end_time)
        })();

        // If the encoder finished the file it is complete and playable, even
        // when the recording ended early because of `error`.
        let mut error = error;
        let plan: &OutputPlan = &self.session.output;
        let (path, complete) = match written {
            Ok(()) => match (self.finalize_recording)(&plan.partial_path, &plan.final_path) {
                Ok(()) => {
                    output::discard(plan);
                    (Some(plan.final_path.clone()), true)
                }
                Err(conversion_error) => {
                    let _ = std::fs::remove_file(&plan.final_path);
                    self.shared.warn(format!(
                        "The recording was kept in its original format because converting it \
                         to MP4 failed: {conversion_error}"
                    ));
                    (output::keep_fallback(plan), true)
                }
            },
            Err(write_error) => {
                error.get_or_insert(write_error);
                (output::keep_incomplete(plan), false)
            }
        };

        let _ = self.shared.transition(RecordingEvent::Finished);
        RecordingOutcome {
            path,
            duration_ms: (end_time * 1000.0) as u64,
            complete,
            error,
            warnings: self.shared.warnings.lock().clone(),
        }
    }
}
