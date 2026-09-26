//! Platform backends.
//!
//! Each supported OS implements [`Platform`], creating the capture and
//! encoding objects that the platform-independent recorder drives. The
//! implementation for the current OS is exported as [`Current`].
//!
//! | OS      | Status                                                       |
//! |---------|--------------------------------------------------------------|
//! | macOS   | ScreenCaptureKit, AVFoundation, AudioToolbox, VideoToolbox   |
//! | Windows | Not implemented yet — reports itself as unsupported          |
//! | Linux   | Not implemented yet — reports itself as unsupported          |

use std::path::Path;

use crate::capabilities::PlatformCapabilities;
use crate::config::{CaptureSource, Size};
use crate::devices::{CameraInfo, DisplayInfo, MicrophoneInfo, WindowInfo};
use crate::error::AppResult;
use crate::permissions::{PermissionKind, PermissionState, PermissionStatus};
use crate::recording::capture::{
    CameraCapture, EncoderSettings, MicrophoneCapture, ScreenCapture, ScreenCaptureRequest,
    SystemAudioCapture, VideoEncoder,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub type Current = macos::MacPlatform;

#[cfg(not(target_os = "macos"))]
mod unimplemented;
#[cfg(not(target_os = "macos"))]
pub type Current = unimplemented::UnimplementedPlatform;

pub trait Platform {
    fn capabilities() -> PlatformCapabilities;

    fn permissions() -> PermissionState;
    /// Prompts the user if the OS allows it; blocks until they answer.
    fn request_permission(kind: PermissionKind) -> PermissionStatus;
    fn open_permission_settings(kind: PermissionKind) -> AppResult<()>;

    fn displays() -> AppResult<Vec<DisplayInfo>>;
    fn windows() -> AppResult<Vec<WindowInfo>>;
    fn cameras() -> AppResult<Vec<CameraInfo>>;
    fn microphones() -> AppResult<Vec<MicrophoneInfo>>;

    /// Native pixel size of a capture source.
    fn source_size(source: &CaptureSource) -> AppResult<Size>;

    fn screen_capture(request: ScreenCaptureRequest) -> AppResult<Box<dyn ScreenCapture>>;
    fn camera_capture(device_id: &str) -> AppResult<Box<dyn CameraCapture>>;
    fn microphone_capture(device_id: &str) -> AppResult<Box<dyn MicrophoneCapture>>;
    fn system_audio_capture() -> AppResult<Box<dyn SystemAudioCapture>>;
    fn encoder(settings: &EncoderSettings) -> AppResult<Box<dyn VideoEncoder>>;

    /// Decodes an audio file to 48 kHz interleaved stereo.
    fn decode_audio_file(path: &Path) -> AppResult<Vec<f32>>;

    /// Current time on the clock used to timestamp captured media, in seconds.
    fn host_time() -> f64;
}
