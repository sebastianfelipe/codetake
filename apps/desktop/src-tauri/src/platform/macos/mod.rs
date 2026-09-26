//! macOS backend: ScreenCaptureKit for screen and system audio, AVFoundation
//! for camera, microphone and encoding, AudioToolbox for music decoding.

mod audio_file;
mod capture_session;
mod devices;
mod encoder;
mod frame;
mod permissions;
mod remux;
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

impl Platform for MacPlatform {
    /// Fragmented QuickTime movie; see `encoder.rs` for why.
    const INTERMEDIATE_EXTENSION: &'static str = "mov";

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

    fn camera_capture(device_id: &str) -> AppResult<Box<dyn CameraCapture>> {
        Ok(Box::new(capture_session::MacCameraCapture::new(device_id)))
    }

    fn microphone_capture(device_id: &str) -> AppResult<Box<dyn MicrophoneCapture>> {
        Ok(Box::new(capture_session::MacMicrophoneCapture::new(
            device_id,
        )))
    }

    fn system_audio_capture() -> AppResult<Box<dyn SystemAudioCapture>> {
        Ok(Box::new(screen::MacSystemAudioCapture::default()))
    }

    fn encoder(settings: &EncoderSettings) -> AppResult<Box<dyn VideoEncoder>> {
        Ok(Box::new(encoder::MacEncoder::new(settings)?))
    }

    fn finalize_recording(intermediate: &Path, destination: &Path) -> AppResult<()> {
        remux::to_mp4(intermediate, destination)
    }

    fn decode_audio_file(path: &Path) -> AppResult<Vec<f32>> {
        audio_file::decode(path)
    }

    fn host_time() -> f64 {
        util::host_time()
    }
}
