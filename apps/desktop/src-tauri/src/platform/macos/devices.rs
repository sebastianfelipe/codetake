//! Enumerates displays, windows, cameras and microphones on macOS.

use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVCaptureDevice, AVCaptureDeviceDiscoverySession, AVCaptureDevicePosition, AVCaptureDeviceType,
    AVCaptureDeviceTypeBuiltInWideAngleCamera, AVCaptureDeviceTypeContinuityCamera,
    AVCaptureDeviceTypeExternal, AVCaptureDeviceTypeMicrophone, AVMediaType, AVMediaTypeAudio,
    AVMediaTypeVideo,
};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayCopyDisplayMode, CGDisplayIsBuiltin, CGDisplayMode, CGMainDisplayID,
};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{SCDisplay, SCShareableContent, SCWindow};

use super::permissions;
use super::util::{describe_error, is_at_least, wait_for};
use crate::config::{CaptureSource, Size};
use crate::devices::{CameraInfo, DisplayInfo, MicrophoneInfo, WindowInfo};
use crate::error::{AppError, AppResult};
use crate::permissions::PermissionKind;

/// A snapshot of what ScreenCaptureKit can capture.
pub struct ShareableContent(pub Retained<SCShareableContent>);

// SAFETY: SCShareableContent is an immutable snapshot.
unsafe impl Send for ShareableContent {}

pub fn shareable_content() -> AppResult<ShareableContent> {
    permissions::ensure(PermissionKind::ScreenRecording)?;
    let result = wait_for(Duration::from_secs(10), |tx| {
        let handler = RcBlock::new(
            move |content: *mut SCShareableContent, error: *mut NSError| {
                // SAFETY: ScreenCaptureKit passes either a valid content object or an error.
                let result = unsafe {
                    match Retained::retain(content) {
                        Some(content) => Ok(ShareableContent(content)),
                        None => Err(error
                            .as_ref()
                            .map(describe_error)
                            .unwrap_or_else(|| "no shareable content".into())),
                    }
                };
                let _ = tx.send(result);
            },
        );
        // SAFETY: the block is 'static and called once.
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
    });
    match result {
        Some(Ok(content)) => Ok(content),
        Some(Err(message)) => Err(AppError::ScreenCapture(message)),
        None => Err(AppError::ScreenCapture(
            "timed out listing screens and windows".into(),
        )),
    }
}

struct DisplayMode {
    pixel_size: Size,
    point_width: usize,
    refresh_rate: Option<f64>,
}

fn display_mode(id: CGDirectDisplayID) -> Option<DisplayMode> {
    let mode = CGDisplayCopyDisplayMode(id)?;
    let pixel_size = Size::new(
        CGDisplayMode::pixel_width(Some(&mode)) as u32,
        CGDisplayMode::pixel_height(Some(&mode)) as u32,
    );
    let refresh = CGDisplayMode::refresh_rate(Some(&mode));
    Some(DisplayMode {
        pixel_size,
        point_width: CGDisplayMode::width(Some(&mode)),
        refresh_rate: (refresh > 0.0).then_some(refresh),
    })
}

/// Points-to-pixels scale factor of a display (2.0 on Retina displays).
fn backing_scale(id: CGDirectDisplayID) -> f64 {
    display_mode(id)
        .filter(|m| m.point_width > 0)
        .map_or(1.0, |m| {
            f64::from(m.pixel_size.width) / m.point_width as f64
        })
}

fn display_info(display: &SCDisplay, index: usize, main: CGDirectDisplayID) -> DisplayInfo {
    // SAFETY: plain property reads on a valid SCDisplay.
    let (id, point_w, point_h) =
        unsafe { (display.displayID(), display.width(), display.height()) };
    let mode = display_mode(id);
    let size = mode
        .as_ref()
        .map_or(Size::new(point_w as u32, point_h as u32), |m| m.pixel_size);
    let name = if CGDisplayIsBuiltin(id) {
        "Built-in Display".to_string()
    } else {
        format!("Display {}", index + 1)
    };
    DisplayInfo {
        id,
        name,
        width: size.width,
        height: size.height,
        refresh_rate: mode.and_then(|m| m.refresh_rate),
        is_primary: id == main,
    }
}

pub fn displays() -> AppResult<Vec<DisplayInfo>> {
    let content = shareable_content()?;
    let main = CGMainDisplayID();
    // SAFETY: reading the display list of a valid snapshot.
    let list = unsafe { content.0.displays() };
    let mut displays: Vec<DisplayInfo> = list
        .iter()
        .enumerate()
        .map(|(i, d)| display_info(&d, i, main))
        .collect();
    displays.sort_by_key(|d| !d.is_primary);
    Ok(displays)
}

/// Windows that make sense to record: on screen, normal level, titled,
/// reasonably sized and not belonging to CodeTake itself.
pub fn capturable_windows(content: &ShareableContent) -> Vec<Retained<SCWindow>> {
    let own_pid = std::process::id() as i32;
    // SAFETY: property reads on valid ScreenCaptureKit objects.
    unsafe {
        content
            .0
            .windows()
            .iter()
            .filter(|w| {
                let frame = w.frame();
                let owner_pid = w.owningApplication().map(|a| a.processID());
                w.isOnScreen()
                    && w.windowLayer() == 0
                    && w.title().is_some_and(|t| !t.to_string().trim().is_empty())
                    && owner_pid.is_some_and(|pid| pid != own_pid)
                    && frame.size.width >= 120.0
                    && frame.size.height >= 80.0
            })
            .collect()
    }
}

pub fn windows() -> AppResult<Vec<WindowInfo>> {
    let content = shareable_content()?;
    let scale = backing_scale(CGMainDisplayID());
    // SAFETY: property reads on valid ScreenCaptureKit objects.
    let windows = capturable_windows(&content)
        .iter()
        .map(|w| unsafe {
            let frame = w.frame();
            WindowInfo {
                id: w.windowID(),
                title: w.title().map(|t| t.to_string()).unwrap_or_default(),
                app_name: w
                    .owningApplication()
                    .map(|a| a.applicationName().to_string())
                    .unwrap_or_default(),
                width: (frame.size.width * scale).round() as u32,
                height: (frame.size.height * scale).round() as u32,
            }
        })
        .collect();
    Ok(windows)
}

pub fn source_size(source: &CaptureSource) -> AppResult<Size> {
    match source {
        CaptureSource::Display { id } => display_mode(*id).map(|m| m.pixel_size).ok_or_else(|| {
            AppError::DeviceUnavailable("the selected display is not connected".into())
        }),
        CaptureSource::Window { id } => windows()?
            .into_iter()
            .find(|w| w.id == *id)
            .map(|w| Size::new(w.width, w.height))
            .ok_or_else(|| {
                AppError::DeviceUnavailable("the selected window is no longer open".into())
            }),
    }
}

fn discover(types: &[&AVCaptureDeviceType], media: &AVMediaType) -> Vec<Retained<AVCaptureDevice>> {
    let types = NSArray::from_slice(types);
    // SAFETY: valid device type constants and media type.
    unsafe {
        AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
            &types,
            Some(media),
            AVCaptureDevicePosition::Unspecified,
        )
        .devices()
        .to_vec()
    }
}

#[allow(deprecated)]
fn camera_types() -> Vec<&'static AVCaptureDeviceType> {
    // SAFETY: framework constants; the macOS 14 names are only read on macOS 14+.
    unsafe {
        if is_at_least(14) {
            vec![
                AVCaptureDeviceTypeBuiltInWideAngleCamera,
                AVCaptureDeviceTypeExternal,
                AVCaptureDeviceTypeContinuityCamera,
            ]
        } else {
            vec![
                AVCaptureDeviceTypeBuiltInWideAngleCamera,
                objc2_av_foundation::AVCaptureDeviceTypeExternalUnknown,
            ]
        }
    }
}

#[allow(deprecated)]
fn microphone_types() -> Vec<&'static AVCaptureDeviceType> {
    // SAFETY: framework constants; the macOS 14 names are only read on macOS 14+.
    unsafe {
        if is_at_least(14) {
            vec![AVCaptureDeviceTypeMicrophone, AVCaptureDeviceTypeExternal]
        } else {
            vec![
                objc2_av_foundation::AVCaptureDeviceTypeBuiltInMicrophone,
                objc2_av_foundation::AVCaptureDeviceTypeExternalUnknown,
            ]
        }
    }
}

fn default_device_id(media: &AVMediaType) -> Option<String> {
    // SAFETY: valid media type constant.
    unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media) }
        .map(|d| unsafe { d.uniqueID() }.to_string())
}

fn list_devices(
    types: &[&AVCaptureDeviceType],
    media: &AVMediaType,
) -> Vec<(String, String, bool)> {
    let default = default_device_id(media);
    let mut seen = std::collections::HashSet::new();
    discover(types, media)
        .iter()
        .filter_map(|device| {
            // SAFETY: property reads on a valid capture device.
            let (id, name) = unsafe {
                (
                    device.uniqueID().to_string(),
                    device.localizedName().to_string(),
                )
            };
            seen.insert(id.clone()).then(|| {
                let is_default = default.as_deref() == Some(id.as_str());
                (id, name, is_default)
            })
        })
        .collect()
}

pub fn cameras() -> AppResult<Vec<CameraInfo>> {
    // SAFETY: framework constant.
    let media = unsafe { AVMediaTypeVideo }
        .ok_or_else(|| AppError::Camera("AVFoundation is unavailable".into()))?;
    Ok(list_devices(&camera_types(), media)
        .into_iter()
        .map(|(id, name, is_default)| CameraInfo {
            id,
            name,
            is_default,
        })
        .collect())
}

pub fn microphones() -> AppResult<Vec<MicrophoneInfo>> {
    // SAFETY: framework constant.
    let media = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| AppError::Microphone("AVFoundation is unavailable".into()))?;
    Ok(list_devices(&microphone_types(), media)
        .into_iter()
        .map(|(id, name, is_default)| MicrophoneInfo {
            id,
            name,
            is_default,
        })
        .collect())
}

/// Looks up a capture device by its unique ID.
pub fn capture_device(id: &str) -> Option<Retained<AVCaptureDevice>> {
    let id = objc2_foundation::NSString::from_str(id);
    // SAFETY: valid NSString argument.
    unsafe { AVCaptureDevice::deviceWithUniqueID(&id) }
}
