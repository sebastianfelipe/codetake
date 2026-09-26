//! Recorder tests against a fake platform.
//!
//! The fake captures deliver synthetic frames and audio from background
//! threads, and the fake encoder records what it receives, so the whole
//! pipeline — threads, clock, mixer, pause handling, error recovery and file
//! finalization — runs for real without touching capture hardware.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::capture::*;
use super::recorder::{self, RecorderEvents, RecordingOutcome, RecordingStatus, StartRequest};
use super::state::RecordingState;
use crate::capabilities::{macos_capabilities, OsVersion, PlatformCapabilities};
use crate::config::*;
use crate::devices::*;
use crate::error::{AppError, AppResult};
use crate::permissions::*;
use crate::platform::Platform;
use crate::video::compositor::{BgraMut, BgraRef};

#[derive(Default, Clone)]
struct Behaviour {
    no_screen_frames: bool,
    screen_error_after: Option<Duration>,
    camera_fails: bool,
    encoder_fails_at_frame: Option<usize>,
    conversion_fails: bool,
}

#[derive(Default)]
struct EncoderLog {
    video_times: Vec<f64>,
    camera_times: Vec<f64>,
    camera_pixel: Option<[u8; 4]>,
    audio_frames: usize,
    audio_times: Vec<f64>,
    finished_at: Option<f64>,
    overlay_pixel: Option<[u8; 4]>,
}

fn behaviour() -> &'static Mutex<Behaviour> {
    static B: OnceLock<Mutex<Behaviour>> = OnceLock::new();
    B.get_or_init(Default::default)
}

fn log() -> &'static Mutex<EncoderLog> {
    static L: OnceLock<Mutex<EncoderLog>> = OnceLock::new();
    L.get_or_init(Default::default)
}

/// The fakes use global state, so tests must not run concurrently.
fn serial() -> parking_lot::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Default::default).lock()
}

fn epoch() -> Instant {
    static E: OnceLock<Instant> = OnceLock::new();
    *E.get_or_init(Instant::now)
}

struct SolidFrame {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl SolidFrame {
    fn new(width: usize, height: usize, bgra: [u8; 4]) -> Self {
        Self {
            width,
            height,
            data: bgra.repeat(width * height),
        }
    }
}

impl VideoFrame for SolidFrame {
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn read(&self, read: &mut dyn FnMut(BgraRef<'_>)) {
        read(BgraRef {
            data: &self.data,
            width: self.width,
            height: self.height,
            stride: self.width * 4,
        });
    }
}

/// Runs `tick` every `interval` on a background thread until stopped.
struct Ticker {
    running: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Ticker {
    fn start(interval: Duration, mut tick: impl FnMut(Duration) -> bool + Send + 'static) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let flag = running.clone();
        let thread = thread::spawn(move || {
            let started = Instant::now();
            while flag.load(Ordering::Acquire) {
                if !tick(started.elapsed()) {
                    break;
                }
                thread::sleep(interval);
            }
        });
        Self {
            running,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct FakeScreen(Option<Ticker>);

impl ScreenCapture for FakeScreen {
    fn start(&mut self, sink: Arc<dyn VideoSink>) -> AppResult<()> {
        let b = behaviour().lock().clone();
        if b.no_screen_frames {
            return Ok(());
        }
        let frame: SharedFrame = Arc::new(SolidFrame::new(64, 36, [10, 20, 30, 255]));
        self.0 = Some(Ticker::start(Duration::from_millis(10), move |elapsed| {
            if b.screen_error_after.is_some_and(|after| elapsed > after) {
                sink.error(AppError::ScreenCapture("display disconnected".into()));
                return false;
            }
            sink.frame(frame.clone(), FakePlatform::host_time());
            true
        }));
        Ok(())
    }
    fn stop(&mut self) {
        if let Some(mut t) = self.0.take() {
            t.stop();
        }
    }
}

struct FakeCamera(Option<Ticker>);

impl CameraCapture for FakeCamera {
    fn start(&mut self, sink: Arc<dyn VideoSink>) -> AppResult<()> {
        if behaviour().lock().camera_fails {
            return Err(AppError::DeviceUnavailable("camera unplugged".into()));
        }
        let frame: SharedFrame = Arc::new(SolidFrame::new(32, 24, [0, 255, 0, 255]));
        self.0 = Some(Ticker::start(Duration::from_millis(30), move |_| {
            sink.frame(frame.clone(), FakePlatform::host_time());
            true
        }));
        Ok(())
    }
    fn stop(&mut self) {
        if let Some(mut t) = self.0.take() {
            t.stop();
        }
    }
}

struct FakeMicrophone(Option<Ticker>);

impl MicrophoneCapture for FakeMicrophone {
    fn start(&mut self, sink: Arc<dyn AudioSink>) -> AppResult<()> {
        // 10 ms buffers of a constant signal.
        self.0 = Some(Ticker::start(Duration::from_millis(10), move |_| {
            sink.audio(FakePlatform::host_time(), &[0.25; 480 * 2]);
            true
        }));
        Ok(())
    }
    fn stop(&mut self) {
        if let Some(mut t) = self.0.take() {
            t.stop();
        }
    }
}

struct FakeEncoder {
    path: PathBuf,
    frames: usize,
    camera: bool,
}

impl VideoEncoder for FakeEncoder {
    fn append_video(
        &mut self,
        media_time: f64,
        fill: &mut dyn FnMut(&mut BgraMut<'_>),
    ) -> AppResult<bool> {
        self.frames += 1;
        if self.camera {
            let mut data = vec![0u8; 32 * 24 * 4];
            fill(&mut BgraMut {
                data: &mut data,
                width: 32,
                height: 24,
                stride: 32 * 4,
            });
            let mut log = log().lock();
            log.camera_times.push(media_time);
            log.camera_pixel = Some([data[0], data[1], data[2], data[3]]);
            return Ok(true);
        }
        if behaviour()
            .lock()
            .encoder_fails_at_frame
            .is_some_and(|n| self.frames >= n)
        {
            return Err(AppError::Encoder("simulated failure".into()));
        }
        let (w, h) = (64, 36);
        let mut data = vec![0u8; w * h * 4];
        fill(&mut BgraMut {
            data: &mut data,
            width: w,
            height: h,
            stride: w * 4,
        });
        // Bottom-right overlay area (where the camera goes by default).
        let i = ((h - 6) * w + (w - 6)) * 4;
        let mut log = log().lock();
        log.overlay_pixel = Some([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        log.video_times.push(media_time);
        fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, b"frame"))
            .map_err(AppError::Io)?;
        Ok(true)
    }

    fn append_audio(&mut self, media_time: f64, samples: &[f32]) -> AppResult<()> {
        let mut log = log().lock();
        log.audio_frames += samples.len() / 2;
        log.audio_times.push(media_time);
        Ok(())
    }

    fn finish(&mut self, end_time: f64) -> AppResult<()> {
        log().lock().finished_at = Some(end_time);
        Ok(())
    }
}

struct FakePlatform;

impl Platform for FakePlatform {
    const INTERMEDIATE_EXTENSION: &'static str = "mov";

    fn capabilities() -> PlatformCapabilities {
        macos_capabilities(OsVersion::new(15, 0, 0))
    }
    fn permissions() -> PermissionState {
        PermissionState {
            screen_recording: PermissionStatus::Granted,
            camera: PermissionStatus::Granted,
            microphone: PermissionStatus::Granted,
        }
    }
    fn request_permission(_: PermissionKind) -> PermissionStatus {
        PermissionStatus::Granted
    }
    fn open_permission_settings(_: PermissionKind) -> AppResult<()> {
        Ok(())
    }
    fn displays() -> AppResult<Vec<DisplayInfo>> {
        Ok(Vec::new())
    }
    fn windows() -> AppResult<Vec<WindowInfo>> {
        Ok(Vec::new())
    }
    fn cameras() -> AppResult<Vec<CameraInfo>> {
        Ok(Vec::new())
    }
    fn microphones() -> AppResult<Vec<MicrophoneInfo>> {
        Ok(Vec::new())
    }
    fn source_size(_: &CaptureSource) -> AppResult<Size> {
        Ok(Size::new(64, 36))
    }
    fn screen_capture(_: ScreenCaptureRequest) -> AppResult<Box<dyn ScreenCapture>> {
        Ok(Box::new(FakeScreen(None)))
    }
    fn camera_capture(_: &str) -> AppResult<Box<dyn CameraCapture>> {
        Ok(Box::new(FakeCamera(None)))
    }
    fn microphone_capture(_: &str) -> AppResult<Box<dyn MicrophoneCapture>> {
        Ok(Box::new(FakeMicrophone(None)))
    }
    fn system_audio_capture() -> AppResult<Box<dyn SystemAudioCapture>> {
        Err(AppError::Unsupported("not in tests".into()))
    }
    fn encoder(settings: &EncoderSettings) -> AppResult<Box<dyn VideoEncoder>> {
        fs::write(&settings.path, b"")?;
        Ok(Box::new(FakeEncoder {
            path: settings.path.clone(),
            frames: 0,
            camera: settings
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains("-camera")),
        }))
    }
    fn finalize_recording(intermediate: &Path, destination: &Path) -> AppResult<()> {
        if behaviour().lock().conversion_fails {
            return Err(AppError::Encoder("conversion failed".into()));
        }
        fs::copy(intermediate, destination)?;
        Ok(())
    }
    fn export_recording(_: &crate::export::ExportJob, _: &mut dyn FnMut(f64)) -> AppResult<()> {
        Ok(())
    }
    fn decode_audio_file(_: &Path) -> AppResult<Vec<f32>> {
        Ok(vec![0.1; 4800 * 2])
    }
    fn host_time() -> f64 {
        epoch().elapsed().as_secs_f64()
    }
}

/// Path, completeness and error code of a finished recording.
type Finished = (Option<PathBuf>, bool, Option<String>);

#[derive(Default)]
struct Events {
    statuses: Mutex<Vec<RecordingState>>,
    finished: Mutex<Option<Finished>>,
}

impl RecorderEvents for Events {
    fn status(&self, status: &RecordingStatus) {
        self.statuses.lock().push(status.state);
    }
    fn finished(&self, outcome: &RecordingOutcome) {
        *self.finished.lock() = Some((
            outcome.path.clone(),
            outcome.complete,
            outcome.error.as_ref().map(|e| e.code().to_string()),
        ));
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codetake-recorder-{name}-{}-{}",
        std::process::id(),
        epoch().elapsed().as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn config(dir: &Path) -> RecordingConfig {
    RecordingConfig {
        source: CaptureSource::Display { id: 1 },
        camera: None,
        microphone: Some(MicrophoneConfig {
            device_id: "mic".into(),
        }),
        system_audio: false,
        music: None,
        resolution: Resolution::Source,
        fps: Fps::Thirty,
        output_directory: dir.to_path_buf(),
    }
}

fn reset(b: Behaviour) {
    *behaviour().lock() = b;
    *log().lock() = EncoderLog::default();
}

/// Names of all files (not folders) under `dir`, recursively.
fn files_in(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            names.extend(files_in(&path));
        } else {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn records_and_finalizes_an_mp4() {
    let _guard = serial();
    reset(Behaviour::default());
    let dir = temp_dir("finalize");
    let events = Arc::new(Events::default());

    let handle = recorder::start::<FakePlatform>(
        StartRequest {
            config: config(&dir),
            music_path: None,
        },
        events.clone(),
    )
    .unwrap();
    assert_eq!(handle.state(), RecordingState::Recording);
    thread::sleep(Duration::from_millis(600));
    let outcome = handle.stop().unwrap();

    assert!(
        outcome.complete,
        "{:?}",
        outcome.error.map(|e| e.to_string())
    );
    assert!(outcome.error.is_none());
    let path = outcome.path.unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().ends_with(".mp4"));
    assert!(!path.to_string_lossy().contains("partial"));
    assert_eq!(files_in(&dir).len(), 1);

    let log = log().lock();
    // ~30 fps for 0.6 s.
    assert!(
        log.video_times.len() >= 10,
        "{} frames",
        log.video_times.len()
    );
    assert!(log.video_times.windows(2).all(|w| w[1] > w[0]));
    let end = log.finished_at.unwrap();
    assert!((end - 0.6).abs() < 0.2, "ended at {end}");
    // The audio track covers the whole recording (flushed on stop).
    let audio_seconds = log.audio_frames as f64 / 48_000.0;
    assert!(
        (audio_seconds - end).abs() < 0.03,
        "{audio_seconds} vs {end}"
    );
    drop(log);
    assert!(events.finished.lock().is_some());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeps_a_playable_movie_when_mp4_conversion_fails() {
    let _guard = serial();
    reset(Behaviour {
        conversion_fails: true,
        ..Behaviour::default()
    });
    let dir = temp_dir("conversion");
    let handle = recorder::start::<FakePlatform>(
        StartRequest {
            config: config(&dir),
            music_path: None,
        },
        Arc::new(Events::default()),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(200));
    let outcome = handle.stop().unwrap();
    assert!(outcome.complete);
    assert!(outcome.error.is_none());
    let path = outcome.path.unwrap();
    assert!(path.to_string_lossy().ends_with(".mov"), "{path:?}");
    assert!(path.exists());
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(files_in(&dir).len(), 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pausing_removes_time_from_the_recording() {
    let _guard = serial();
    reset(Behaviour::default());
    let dir = temp_dir("pause");
    let handle = recorder::start::<FakePlatform>(
        StartRequest {
            config: config(&dir),
            music_path: None,
        },
        Arc::new(Events::default()),
    )
    .unwrap();

    thread::sleep(Duration::from_millis(300));
    handle.pause().unwrap();
    assert_eq!(handle.state(), RecordingState::Paused);
    assert!(handle.pause().is_err());
    thread::sleep(Duration::from_millis(500));
    handle.resume().unwrap();
    thread::sleep(Duration::from_millis(300));
    let outcome = handle.stop().unwrap();

    assert!(outcome.complete);
    let duration = outcome.duration_ms as f64 / 1000.0;
    assert!((duration - 0.6).abs() < 0.2, "duration {duration}");
    let log = log().lock();
    let largest_gap = log
        .video_times
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(0.0, f64::max);
    assert!(largest_gap < 0.15, "gap of {largest_gap}s in the video");
    drop(log);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn records_the_camera_to_its_own_file_in_sync_with_the_screen() {
    let _guard = serial();
    reset(Behaviour::default());
    let dir = temp_dir("camera");
    let mut config = config(&dir);
    let overlay = CameraOverlay {
        shape: OverlayShape::Rectangle,
        size: 0.5,
        x: 1.0,
        y: 1.0,
    };
    config.camera = Some(CameraConfig {
        device_id: "cam".into(),
        overlay,
    });
    let handle = recorder::start::<FakePlatform>(
        StartRequest {
            config,
            music_path: None,
        },
        Arc::new(Events::default()),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(400));
    let outcome = handle.stop().unwrap();

    // The screen file contains only the screen...
    assert_eq!(log().lock().overlay_pixel, Some([10, 20, 30, 255]));
    // ...and the camera has its own file, with the screen's timestamps.
    let log = log().lock();
    assert!(
        log.camera_times.len() > 3,
        "{} camera frames",
        log.camera_times.len()
    );
    assert!(log.camera_times.iter().all(|t| log.video_times.contains(t)));
    assert_eq!(log.camera_pixel, Some([0, 255, 0, 255]));
    drop(log);

    let screen = outcome.path.unwrap();
    let camera = outcome.camera_path.unwrap();
    assert!(screen.to_string_lossy().ends_with("-screen.mp4"));
    assert!(camera.to_string_lossy().ends_with("-camera.mp4"));
    assert!(screen.parent().unwrap().ends_with("raw"));
    assert!(camera.exists());
    assert_eq!(outcome.overlay, Some(overlay));
    assert!(outcome.export_path.to_string_lossy().ends_with(".mp4"));
    assert!(
        !outcome.export_path.exists(),
        "the review step exports the final video"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn encoder_failure_stops_the_recording_and_keeps_the_file() {
    let _guard = serial();
    reset(Behaviour {
        encoder_fails_at_frame: Some(6),
        ..Behaviour::default()
    });
    let dir = temp_dir("encoder-failure");
    let events = Arc::new(Events::default());
    let handle = recorder::start::<FakePlatform>(
        StartRequest {
            config: config(&dir),
            music_path: None,
        },
        events.clone(),
    )
    .unwrap();

    assert!(wait_until(Duration::from_secs(3), || events
        .finished
        .lock()
        .is_some()));
    assert!(handle.is_finished());
    let (path, _complete, error) = events.finished.lock().clone().unwrap();
    assert_eq!(error.as_deref(), Some("encoder"));
    let path = path.expect("written media must be kept");
    assert!(path.exists());
    assert!(handle.stop().is_err(), "already stopped");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn losing_the_screen_stops_and_saves_the_recording() {
    let _guard = serial();
    reset(Behaviour {
        screen_error_after: Some(Duration::from_millis(250)),
        ..Behaviour::default()
    });
    let dir = temp_dir("screen-lost");
    let events = Arc::new(Events::default());
    let _handle = recorder::start::<FakePlatform>(
        StartRequest {
            config: config(&dir),
            music_path: None,
        },
        events.clone(),
    )
    .unwrap();

    assert!(wait_until(Duration::from_secs(3), || events
        .finished
        .lock()
        .is_some()));
    let (path, complete, error) = events.finished.lock().clone().unwrap();
    assert_eq!(error.as_deref(), Some("screen_capture"));
    // The encoder itself was fine, so the file is finalized normally.
    assert!(complete);
    assert!(path.unwrap().exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn start_fails_cleanly_without_screen_frames() {
    let _guard = serial();
    reset(Behaviour {
        no_screen_frames: true,
        ..Behaviour::default()
    });
    let dir = temp_dir("no-frames");
    let result = recorder::start::<FakePlatform>(
        StartRequest {
            config: config(&dir),
            music_path: None,
        },
        Arc::new(Events::default()),
    );
    assert!(matches!(result, Err(AppError::ScreenCapture(_))));
    assert!(
        files_in(&dir).is_empty(),
        "no partial file may be left behind"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn start_fails_when_an_enabled_camera_is_unavailable() {
    let _guard = serial();
    reset(Behaviour {
        camera_fails: true,
        ..Behaviour::default()
    });
    let dir = temp_dir("camera-missing");
    let mut config = config(&dir);
    config.camera = Some(CameraConfig {
        device_id: "cam".into(),
        overlay: CameraOverlay::default(),
    });
    let result = recorder::start::<FakePlatform>(
        StartRequest {
            config,
            music_path: None,
        },
        Arc::new(Events::default()),
    );
    assert!(matches!(result, Err(AppError::DeviceUnavailable(_))));
    assert!(files_in(&dir).is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn music_without_a_resolved_file_is_rejected() {
    let _guard = serial();
    reset(Behaviour::default());
    let dir = temp_dir("music");
    let mut config = config(&dir);
    config.music = Some(MusicConfig {
        track_id: "missing".into(),
        volume: 0.5,
    });
    let result = recorder::start::<FakePlatform>(
        StartRequest {
            config,
            music_path: None,
        },
        Arc::new(Events::default()),
    );
    assert!(matches!(result, Err(AppError::Music(_))));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn records_with_music_only_audio() {
    let _guard = serial();
    reset(Behaviour::default());
    let dir = temp_dir("music-only");
    let mut config = config(&dir);
    config.microphone = None;
    config.music = Some(MusicConfig {
        track_id: "coding-01".into(),
        volume: 0.5,
    });
    let handle = recorder::start::<FakePlatform>(
        StartRequest {
            config,
            music_path: Some(PathBuf::from("/music/coding-01.m4a")),
        },
        Arc::new(Events::default()),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(300));
    let outcome = handle.stop().unwrap();
    assert!(outcome.complete);
    assert!(log().lock().audio_frames > 0);
    fs::remove_dir_all(dir).unwrap();
}
