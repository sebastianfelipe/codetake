//! Screen and system audio capture with ScreenCaptureKit.

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_core_graphics::kCGColorSpaceSRGB;
use objc2_core_graphics::CGMainDisplayID;
use objc2_core_media::CMTime;
use objc2_core_video::kCVPixelFormatType_32BGRA;
use objc2_foundation::NSArray;
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCRunningApplication, SCStreamConfiguration, SCWindow,
};

use super::devices::{shareable_content, ShareableContent};
use super::stream::{RunningStream, StreamTarget};
use crate::config::CaptureSource;
use crate::error::{AppError, AppResult};
use crate::recording::capture::{
    AudioSink, ScreenCapture, ScreenCaptureRequest, SystemAudioCapture, VideoSink,
};

fn find_display(content: &ShareableContent, id: u32) -> Option<Retained<SCDisplay>> {
    // SAFETY: property reads on a valid snapshot.
    unsafe { content.0.displays().iter().find(|d| d.displayID() == id) }
}

fn find_window(content: &ShareableContent, id: u32) -> Option<Retained<SCWindow>> {
    // SAFETY: property reads on a valid snapshot.
    unsafe { content.0.windows().iter().find(|w| w.windowID() == id) }
}

fn own_application(content: &ShareableContent) -> Vec<Retained<SCRunningApplication>> {
    let pid = std::process::id() as i32;
    // SAFETY: property reads on a valid snapshot.
    unsafe {
        content
            .0
            .applications()
            .iter()
            .filter(|a| a.processID() == pid)
            .collect()
    }
}

/// Builds the content filter for a capture source. CodeTake's own windows
/// are excluded from display captures so the app never records itself.
fn content_filter(source: &CaptureSource) -> AppResult<Retained<SCContentFilter>> {
    let content = shareable_content()?;
    match source {
        CaptureSource::Display { id } => {
            let display = find_display(&content, *id).ok_or_else(|| {
                AppError::DeviceUnavailable("the selected display is not connected".into())
            })?;
            let excluded = NSArray::from_retained_slice(&own_application(&content));
            let no_windows = NSArray::<SCWindow>::new();
            // SAFETY: all arguments are valid ScreenCaptureKit objects.
            Ok(unsafe {
                SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &excluded,
                    &no_windows,
                )
            })
        }
        CaptureSource::Window { id } => {
            let window = find_window(&content, *id).ok_or_else(|| {
                AppError::DeviceUnavailable("the selected window is no longer open".into())
            })?;
            // SAFETY: `window` is a valid SCWindow.
            Ok(unsafe {
                SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
            })
        }
    }
}

pub struct MacScreenCapture {
    request: ScreenCaptureRequest,
    stream: Option<RunningStream>,
}

impl MacScreenCapture {
    pub fn new(request: ScreenCaptureRequest) -> Self {
        Self {
            request,
            stream: None,
        }
    }

    fn configuration(&self) -> Retained<SCStreamConfiguration> {
        let request = &self.request;
        // SAFETY: setting plain properties on a fresh configuration object.
        unsafe {
            let config = SCStreamConfiguration::new();
            config.setWidth(request.output_size.width as usize);
            config.setHeight(request.output_size.height as usize);
            config.setMinimumFrameInterval(CMTime::new(1, request.fps as i32));
            config.setPixelFormat(kCVPixelFormatType_32BGRA);
            config.setShowsCursor(request.show_cursor);
            config.setScalesToFit(true);
            config.setQueueDepth(6);
            // Convert from the display's color space (often Display P3) to
            // sRGB, which matches the BT.709 primaries written to the file.
            config.setColorSpaceName(kCGColorSpaceSRGB);
            config
        }
    }
}

impl ScreenCapture for MacScreenCapture {
    fn start(&mut self, sink: Arc<dyn VideoSink>) -> AppResult<()> {
        let filter = content_filter(&self.request.source)?;
        let config = self.configuration();
        let stream = RunningStream::start(
            &filter,
            &config,
            StreamTarget::Video(sink),
            "dev.codetake.capture.screen",
        )?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.stop();
        }
    }
}

impl Drop for MacScreenCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Captures the audio of every application except CodeTake.
///
/// ScreenCaptureKit only delivers audio as part of a stream with a content
/// filter, so this runs a separate, tiny, low-frame-rate display stream whose
/// video output is ignored.
#[derive(Default)]
pub struct MacSystemAudioCapture {
    stream: Option<RunningStream>,
}

impl SystemAudioCapture for MacSystemAudioCapture {
    fn start(&mut self, sink: Arc<dyn AudioSink>) -> AppResult<()> {
        let content = shareable_content()?;
        let display = find_display(&content, CGMainDisplayID())
            .or_else(|| unsafe { content.0.displays().firstObject() })
            .ok_or_else(|| AppError::SystemAudio("no display available".into()))?;
        let no_windows = NSArray::<SCWindow>::new();
        // SAFETY: all arguments are valid ScreenCaptureKit objects, and
        // configuration setters take plain values.
        let (filter, config) = unsafe {
            let filter = SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display,
                &no_windows,
            );
            let config = SCStreamConfiguration::new();
            config.setCapturesAudio(true);
            config.setSampleRate(48_000);
            config.setChannelCount(2);
            config.setExcludesCurrentProcessAudio(true);
            config.setWidth(2);
            config.setHeight(2);
            config.setMinimumFrameInterval(CMTime::new(1, 1));
            (filter, config)
        };
        let stream = RunningStream::start(
            &filter,
            &config,
            StreamTarget::Audio(sink),
            "dev.codetake.capture.system-audio",
        )
        .map_err(|e| match e {
            AppError::ScreenCapture(message) => AppError::SystemAudio(message),
            other => other,
        })?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.stop();
        }
    }
}

impl Drop for MacSystemAudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
