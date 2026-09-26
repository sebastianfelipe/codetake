//! Tauri commands: the only surface the UI uses to talk to the backend.
//!
//! Commands that touch capture APIs can block (waiting for the OS), so they
//! run on the blocking thread pool rather than the async runtime.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};

use crate::capabilities::PlatformCapabilities;
use crate::config::RecordingConfig;
use crate::devices::{CameraInfo, DisplayInfo, MicrophoneInfo, WindowInfo};
use crate::error::{AppError, AppResult};
use crate::music::{self, MusicTrack};
use crate::output;
use crate::permissions::{PermissionKind, PermissionState, PermissionStatus};
use crate::platform::{Current, Platform};
use crate::recording::preview::{PreviewEvents, PreviewKind, PreviewRequest, PreviewSession};
use crate::recording::recorder::{
    self, RecorderEvents, RecordingHandle, RecordingOutcome, RecordingStatus, StartRequest,
};
use crate::recording::session::SessionSummary;

pub const EVENT_STATUS: &str = "recording://status";
pub const EVENT_FINISHED: &str = "recording://finished";
pub const EVENT_PREVIEW_LEVEL: &str = "preview://level";
pub const EVENT_PREVIEW_ERROR: &str = "preview://error";

#[derive(Default)]
pub struct AppState {
    recording: Mutex<Option<RecordingHandle>>,
    preview: Mutex<Option<PreviewSession>>,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| AppError::InvalidState(format!("background task failed: {e}")))?
}

#[tauri::command]
pub fn get_capabilities() -> PlatformCapabilities {
    Current::capabilities()
}

#[tauri::command]
pub fn get_permissions() -> PermissionState {
    Current::permissions()
}

#[tauri::command]
pub async fn request_permission(kind: PermissionKind) -> AppResult<PermissionStatus> {
    blocking(move || Ok(Current::request_permission(kind))).await
}

#[tauri::command]
pub fn open_permission_settings(kind: PermissionKind) -> AppResult<()> {
    Current::open_permission_settings(kind)
}

#[tauri::command]
pub async fn list_displays() -> AppResult<Vec<DisplayInfo>> {
    blocking(Current::displays).await
}

#[tauri::command]
pub async fn list_windows() -> AppResult<Vec<WindowInfo>> {
    blocking(Current::windows).await
}

#[tauri::command]
pub async fn list_cameras() -> AppResult<Vec<CameraInfo>> {
    blocking(Current::cameras).await
}

#[tauri::command]
pub async fn list_microphones() -> AppResult<Vec<MicrophoneInfo>> {
    blocking(Current::microphones).await
}

fn music_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .resource_dir()
        .map(|dir| dir.join("music"))
        .map_err(|e| AppError::Music(format!("cannot locate bundled music: {e}")))
}

#[tauri::command]
pub fn list_music_tracks(app: AppHandle) -> AppResult<Vec<MusicTrack>> {
    music::load_catalog(&music_dir(&app)?)
}

#[tauri::command]
pub fn default_output_directory(app: AppHandle) -> AppResult<PathBuf> {
    let videos = app
        .path()
        .video_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| AppError::Storage(format!("cannot find the Movies folder: {e}")))?;
    Ok(output::default_output_directory(&videos))
}

/// Converts recordings left behind by a crash; returns the recovered files.
#[tauri::command]
pub async fn recover_recordings(output_directory: PathBuf) -> AppResult<Vec<PathBuf>> {
    blocking(move || {
        Ok(output::recover_partial_recordings(
            &output_directory,
            Current::finalize_recording,
        ))
    })
    .await
}

fn settings_path(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|e| AppError::Storage(format!("cannot locate the settings folder: {e}")))
}

/// Returns the saved settings, or `null` if none have been saved yet. The
/// frontend owns the format and validates it.
#[tauri::command]
pub fn load_settings(app: AppHandle) -> AppResult<Option<serde_json::Value>> {
    let path = settings_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(text) => Ok(serde_json::from_str(&text).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: serde_json::Value) -> AppResult<()> {
    let path = settings_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Write then rename so a crash never leaves a truncated settings file.
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_vec_pretty(&settings).unwrap_or_default(),
    )?;
    fs::rename(temp, path)?;
    Ok(())
}

struct TauriPreviewEvents {
    app: AppHandle,
    frames: Channel<InvokeResponseBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewErrorPayload {
    kind: PreviewKind,
    error: AppError,
}

impl PreviewEvents for TauriPreviewEvents {
    fn frame(&self, kind: PreviewKind, width: usize, height: usize, rgba: Vec<u8>) {
        // Binary message: [kind, 0, width (u16 LE), height (u16 LE), 0, 0] + RGBA.
        let kind_byte = match kind {
            PreviewKind::Screen => 0,
            PreviewKind::Camera => 1,
            PreviewKind::Microphone => 2,
        };
        let mut message = Vec::with_capacity(8 + rgba.len());
        message.extend_from_slice(&[kind_byte, 0]);
        message.extend_from_slice(&(width as u16).to_le_bytes());
        message.extend_from_slice(&(height as u16).to_le_bytes());
        message.extend_from_slice(&[0, 0]);
        message.extend_from_slice(&rgba);
        let _ = self.frames.send(InvokeResponseBody::Raw(message));
    }

    fn level(&self, level: f32) {
        let _ = self.app.emit(EVENT_PREVIEW_LEVEL, level);
    }

    fn error(&self, kind: PreviewKind, error: AppError) {
        log::warn!("preview {kind:?} unavailable: {error}");
        let _ = self
            .app
            .emit(EVENT_PREVIEW_ERROR, &PreviewErrorPayload { kind, error });
    }
}

/// Starts (or restarts) the live preview.
#[tauri::command]
pub async fn start_preview(
    app: AppHandle,
    request: PreviewRequest,
    frames: Channel<InvokeResponseBody>,
) -> AppResult<()> {
    blocking(move || {
        let state = app.state::<AppState>();
        if state
            .recording
            .lock()
            .as_ref()
            .is_some_and(|r| !r.is_finished())
        {
            return Ok(()); // devices are in use by the recording
        }
        // Release the devices before opening them again.
        drop(state.preview.lock().take());
        let events = Arc::new(TauriPreviewEvents {
            app: app.clone(),
            frames,
        });
        let session = PreviewSession::start::<Current>(&request, events);
        *state.preview.lock() = Some(session);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn stop_preview(app: AppHandle) -> AppResult<()> {
    blocking(move || {
        drop(app.state::<AppState>().preview.lock().take());
        Ok(())
    })
    .await
}

struct TauriRecorderEvents(AppHandle);

impl RecorderEvents for TauriRecorderEvents {
    fn status(&self, status: &RecordingStatus) {
        let _ = self.0.emit(EVENT_STATUS, status);
    }

    fn finished(&self, outcome: &RecordingOutcome) {
        let _ = self.0.emit(EVENT_FINISHED, outcome);
    }
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, config: RecordingConfig) -> AppResult<SessionSummary> {
    blocking(move || {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock();
        if recording.as_ref().is_some_and(|r| !r.is_finished()) {
            return Err(AppError::InvalidState(
                "a recording is already running".into(),
            ));
        }
        // The preview holds the camera and microphone; release them first.
        drop(state.preview.lock().take());

        let music_path = match &config.music {
            Some(m) => Some(music::resolve(&music_dir(&app)?, &m.track_id)?),
            None => None,
        };
        let handle = recorder::start::<Current>(
            StartRequest { config, music_path },
            Arc::new(TauriRecorderEvents(app.clone())),
        )?;
        let summary = handle.summary();
        *recording = Some(handle);
        Ok(summary)
    })
    .await
}

fn with_recording<T>(
    app: &AppHandle,
    action: impl FnOnce(&RecordingHandle) -> AppResult<T>,
) -> AppResult<T> {
    let state = app.state::<AppState>();
    let recording = state.recording.lock();
    match recording.as_ref() {
        Some(handle) if !handle.is_finished() => action(handle),
        _ => Err(AppError::InvalidState("not recording".into())),
    }
}

#[tauri::command]
pub async fn pause_recording(app: AppHandle) -> AppResult<()> {
    blocking(move || with_recording(&app, RecordingHandle::pause)).await
}

#[tauri::command]
pub async fn resume_recording(app: AppHandle) -> AppResult<()> {
    blocking(move || with_recording(&app, RecordingHandle::resume)).await
}

/// Stops and finalizes a running recording, if any; used before quitting.
/// Returns whether there was one.
pub fn finish_active_recording(app: &AppHandle) -> bool {
    let handle = app.state::<AppState>().recording.lock().take();
    match handle {
        Some(handle) if !handle.is_finished() => {
            if let Err(error) = handle.stop() {
                log::error!("could not finalize the recording before quitting: {error}");
            }
            true
        }
        _ => false,
    }
}

/// Releases the camera and microphone used by the preview (e.g. when the
/// window is hidden, so the camera light turns off).
pub fn release_preview(app: &AppHandle) {
    drop(app.state::<AppState>().preview.lock().take());
}

/// Whether a recording is currently running.
pub fn is_recording(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .recording
        .lock()
        .as_ref()
        .is_some_and(|r| !r.is_finished())
}

/// Stops the recording and waits until the MP4 is written.
#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> AppResult<RecordingOutcome> {
    blocking(move || {
        let handle = app
            .state::<AppState>()
            .recording
            .lock()
            .take()
            .ok_or_else(|| AppError::InvalidState("not recording".into()))?;
        handle.stop()
    })
    .await
}
