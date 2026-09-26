//! macOS backend: ScreenCaptureKit for screen and system audio, AVFoundation
//! for camera, microphone and encoding, AudioToolbox for music decoding.

mod devices;
mod frame;
mod permissions;
mod screen;
mod stream;
mod util;

use std::path::Path;

use super::Platform;
use crate::capabilities::{macos_capabilities, PlatformCapabilities};
use crate::config::{CaptureSource, Size};
use crate::devices::{CameraInfo, DisplayInfo, MicrophoneInfo, WindowInfo};
use crate::error::AppResult;
use crate::permissions::{PermissionKind, PermissionState, PermissionStatus};
use crate::recording::capture::{
    CameraCapture, EncoderSettings, MicrophoneCapture, ScreenCapture, ScreenCaptureRequest,
    SystemAudioCapture, VideoEncoder,
};

pub struct MacPlatform;

fn not_yet<T>(feature: &str) -> AppResult<T> {
    Err(crate::error::AppError::Unsupported(format!(
        "{feature} is not implemented yet"
    )))
}

impl Platform for MacPlatform {
    fn capabilities() -> PlatformCapabilities {
        macos_capabilities(util::os_version())
    }

    fn permissions() -> PermissionState {
        permissions::state()
    }

    fn request_permission(kind: PermissionKind) -> PermissionStatus {
        permissions::request(kind)
    }

    fn open_permission_settings(kind: PermissionKind) -> AppResult<()> {
        permissions::open_settings(kind)
    }

    fn displays() -> AppResult<Vec<DisplayInfo>> {
        devices::displays()
    }

    fn windows() -> AppResult<Vec<WindowInfo>> {
        devices::windows()
    }

    fn cameras() -> AppResult<Vec<CameraInfo>> {
        devices::cameras()
    }

    fn microphones() -> AppResult<Vec<MicrophoneInfo>> {
        devices::microphones()
    }

    fn source_size(source: &CaptureSource) -> AppResult<Size> {
        devices::source_size(source)
    }

    fn screen_capture(request: ScreenCaptureRequest) -> AppResult<Box<dyn ScreenCapture>> {
        Ok(Box::new(screen::MacScreenCapture::new(request)))
    }

    fn camera_capture(_device_id: &str) -> AppResult<Box<dyn CameraCapture>> {
        not_yet("camera capture")
    }

    fn microphone_capture(_device_id: &str) -> AppResult<Box<dyn MicrophoneCapture>> {
        not_yet("microphone capture")
    }

    fn system_audio_capture() -> AppResult<Box<dyn SystemAudioCapture>> {
        Ok(Box::new(screen::MacSystemAudioCapture::default()))
    }

    fn encoder(_settings: &EncoderSettings) -> AppResult<Box<dyn VideoEncoder>> {
        not_yet("MP4 encoding")
    }

    fn decode_audio_file(_path: &Path) -> AppResult<Vec<f32>> {
        not_yet("music decoding")
    }

    fn host_time() -> f64 {
        util::host_time()
    }
}
