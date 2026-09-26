//! Placeholder backend for platforms without a capture implementation yet.
//!
//! It reports every capability as unsupported (with a reason the UI shows)
//! and refuses to create capture objects. It never fakes a recording.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use super::Platform;
use crate::capabilities::{unimplemented_platform_capabilities, PlatformCapabilities};
use crate::config::{CaptureSource, Size};
use crate::devices::{CameraInfo, DisplayInfo, MicrophoneInfo, WindowInfo};
use crate::error::{AppError, AppResult};
use crate::export::ExportJob;
use crate::permissions::{PermissionKind, PermissionState, PermissionStatus};
use crate::recording::capture::{
    CameraCapture, EncoderSettings, MicrophoneCapture, ScreenCapture, ScreenCaptureRequest,
    SystemAudioCapture, VideoEncoder,
};

pub struct UnimplementedPlatform;

fn unsupported<T>() -> AppResult<T> {
    let caps = UnimplementedPlatform::capabilities();
    Err(AppError::Unsupported(caps.recording.reason.unwrap_or_else(
        || "Recording is not supported on this platform yet.".into(),
    )))
}

impl Platform for UnimplementedPlatform {
    const INTERMEDIATE_EXTENSION: &'static str = "mp4";

    fn capabilities() -> PlatformCapabilities {
        unimplemented_platform_capabilities(std::env::consts::OS, String::new())
    }

    fn permissions() -> PermissionState {
        PermissionState {
            screen_recording: PermissionStatus::NotRequired,
            camera: PermissionStatus::NotRequired,
            microphone: PermissionStatus::NotRequired,
        }
    }

    fn request_permission(_kind: PermissionKind) -> PermissionStatus {
        PermissionStatus::NotRequired
    }

    fn open_permission_settings(_kind: PermissionKind) -> AppResult<()> {
        unsupported()
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

    fn source_size(_source: &CaptureSource) -> AppResult<Size> {
        unsupported()
    }

    fn screen_capture(_request: ScreenCaptureRequest) -> AppResult<Box<dyn ScreenCapture>> {
        unsupported()
    }

    fn camera_capture(_device_id: &str) -> AppResult<Box<dyn CameraCapture>> {
        unsupported()
    }

    fn microphone_capture(_device_id: &str) -> AppResult<Box<dyn MicrophoneCapture>> {
        unsupported()
    }

    fn system_audio_capture() -> AppResult<Box<dyn SystemAudioCapture>> {
        unsupported()
    }

    fn encoder(_settings: &EncoderSettings) -> AppResult<Box<dyn VideoEncoder>> {
        unsupported()
    }

    fn finalize_recording(_intermediate: &Path, _destination: &Path) -> AppResult<()> {
        unsupported()
    }

    fn export_recording(_job: &ExportJob, _progress: &mut dyn FnMut(f64)) -> AppResult<()> {
        unsupported()
    }

    fn decode_audio_file(_path: &Path) -> AppResult<Vec<f32>> {
        unsupported()
    }

    fn host_time() -> f64 {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64()
    }
}
