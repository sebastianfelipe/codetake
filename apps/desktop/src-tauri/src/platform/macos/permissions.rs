//! macOS privacy permissions (TCC): Screen Recording, Camera, Microphone.

use std::process::Command;
use std::time::Duration;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

use super::util::wait_for;
use crate::error::{AppError, AppResult};
use crate::permissions::{PermissionKind, PermissionState, PermissionStatus};

fn media_type(kind: PermissionKind) -> Option<&'static AVMediaType> {
    // SAFETY: reading framework-provided constant strings.
    unsafe {
        match kind {
            PermissionKind::Camera => AVMediaTypeVideo,
            PermissionKind::Microphone => AVMediaTypeAudio,
            PermissionKind::ScreenRecording => None,
        }
    }
}

fn capture_device_status(kind: PermissionKind) -> PermissionStatus {
    let Some(media) = media_type(kind) else {
        return PermissionStatus::NotRequired;
    };
    // SAFETY: `media` is a valid AVMediaType constant.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    match status {
        AVAuthorizationStatus::Authorized => PermissionStatus::Granted,
        AVAuthorizationStatus::Denied => PermissionStatus::Denied,
        AVAuthorizationStatus::Restricted => PermissionStatus::Restricted,
        _ => PermissionStatus::NotDetermined,
    }
}

/// macOS only tells us whether Screen Recording is granted; "not yet asked"
/// and "denied" look the same, so we report `NotDetermined` and let the UI
/// offer both the prompt and System Settings.
fn screen_recording_status() -> PermissionStatus {
    if CGPreflightScreenCaptureAccess() {
        PermissionStatus::Granted
    } else {
        PermissionStatus::NotDetermined
    }
}

pub fn status(kind: PermissionKind) -> PermissionStatus {
    match kind {
        PermissionKind::ScreenRecording => screen_recording_status(),
        _ => capture_device_status(kind),
    }
}

pub fn state() -> PermissionState {
    PermissionState {
        screen_recording: status(PermissionKind::ScreenRecording),
        camera: status(PermissionKind::Camera),
        microphone: status(PermissionKind::Microphone),
    }
}

pub fn request(kind: PermissionKind) -> PermissionStatus {
    match kind {
        PermissionKind::ScreenRecording => {
            // Shows the system prompt the first time; afterwards the user
            // must change the setting in System Settings.
            if CGRequestScreenCaptureAccess() {
                PermissionStatus::Granted
            } else {
                screen_recording_status()
            }
        }
        _ => {
            let Some(media) = media_type(kind) else {
                return PermissionStatus::NotRequired;
            };
            if status(kind) != PermissionStatus::NotDetermined {
                return status(kind);
            }
            // The user may take a while to answer the dialog.
            let granted = wait_for(Duration::from_secs(600), |tx| {
                let handler = RcBlock::new(move |granted: Bool| {
                    let _ = tx.send(granted.as_bool());
                });
                // SAFETY: `media` is a valid constant and the block is 'static.
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler)
                };
            });
            match granted {
                Some(true) => PermissionStatus::Granted,
                _ => status(kind),
            }
        }
    }
}

pub fn open_settings(kind: PermissionKind) -> AppResult<()> {
    let pane = match kind {
        PermissionKind::ScreenRecording => "Privacy_ScreenCapture",
        PermissionKind::Camera => "Privacy_Camera",
        PermissionKind::Microphone => "Privacy_Microphone",
    };
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
    let status = Command::new("open").arg(&url).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::InvalidState(format!(
            "could not open System Settings ({url})"
        )))
    }
}

/// Fails with a permission error if `kind` is not usable.
pub fn ensure(kind: PermissionKind) -> AppResult<()> {
    if status(kind).is_usable() {
        Ok(())
    } else {
        Err(AppError::PermissionDenied(kind))
    }
}
