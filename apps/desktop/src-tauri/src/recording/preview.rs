//! Live preview: a screen thumbnail, the camera image and the microphone
//! level.
//!
//! Before recording, [`PreviewSession`] runs its own captures at low frame
//! rates and small sizes. Each part is best effort: a missing permission or
//! device is reported through [`PreviewEvents::error`] without stopping the
//! others. While recording, the devices belong to the recorder, which feeds
//! the same thumbnails from its own frames through a [`FrameThrottle`] so the
//! preview stays live.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::capture::{
    AudioSink, CameraCapture, MicrophoneCapture, ScreenCapture, ScreenCaptureRequest, SharedFrame,
    VideoSink,
};
use crate::audio::peak_level;
use crate::config::{CaptureSource, Size};
use crate::error::AppError;
use crate::platform::Platform;
use crate::video::compositor::thumbnail_rgba;

const SCREEN_INTERVAL: Duration = Duration::from_millis(500);
const CAMERA_INTERVAL: Duration = Duration::from_millis(66);
const LEVEL_INTERVAL: Duration = Duration::from_millis(50);
const SCREEN_THUMBNAIL: (usize, usize) = (960, 540);
const CAMERA_THUMBNAIL: (usize, usize) = (320, 320);

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub source: Option<CaptureSource>,
    pub camera_id: Option<String>,
    pub microphone_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewKind {
    Screen,
    Camera,
    Microphone,
}

pub trait PreviewEvents: Send + Sync + 'static {
    /// A thumbnail as tightly packed RGBA.
    fn frame(&self, kind: PreviewKind, width: usize, height: usize, rgba: Vec<u8>);
    fn level(&self, level: f32);
    fn error(&self, kind: PreviewKind, error: AppError);
}

/// Turns full-size frames into preview thumbnails, at most one per interval.
pub struct FrameThrottle {
    kind: PreviewKind,
    interval: Duration,
    max_size: (usize, usize),
    last: Mutex<Option<Instant>>,
}

impl FrameThrottle {
    pub fn screen() -> Self {
        Self::new(PreviewKind::Screen, SCREEN_INTERVAL, SCREEN_THUMBNAIL)
    }

    pub fn camera() -> Self {
        Self::new(PreviewKind::Camera, CAMERA_INTERVAL, CAMERA_THUMBNAIL)
    }

    fn new(kind: PreviewKind, interval: Duration, max_size: (usize, usize)) -> Self {
        Self {
            kind,
            interval,
            max_size,
            last: Mutex::new(None),
        }
    }

    /// Calls `emit` with a downscaled RGBA copy of `frame`, unless a
    /// thumbnail was produced less than one interval ago.
    pub fn offer(
        &self,
        frame: &SharedFrame,
        emit: &mut dyn FnMut(PreviewKind, usize, usize, Vec<u8>),
    ) {
        {
            let mut last = self.last.lock();
            if last.is_some_and(|t| t.elapsed() < self.interval) {
                return;
            }
            *last = Some(Instant::now());
        }
        frame.read(&mut |src| {
            let (w, h, rgba) = thumbnail_rgba(src, self.max_size.0, self.max_size.1);
            emit(self.kind, w, h, rgba);
        });
    }
}

/// Sends the preview session's captured frames as thumbnails.
struct ThrottledFrames {
    throttle: FrameThrottle,
    events: Arc<dyn PreviewEvents>,
}

impl VideoSink for ThrottledFrames {
    fn frame(&self, frame: SharedFrame, _host_time: f64) {
        self.throttle.offer(&frame, &mut |kind, w, h, rgba| {
            self.events.frame(kind, w, h, rgba)
        });
    }

    fn error(&self, error: AppError) {
        self.events.error(self.throttle.kind, error);
    }
}

struct LevelMeter {
    window: Mutex<(Option<Instant>, f32)>,
    events: Arc<dyn PreviewEvents>,
}

impl AudioSink for LevelMeter {
    fn audio(&self, _host_time: f64, samples: &[f32]) {
        let mut window = self.window.lock();
        window.1 = window.1.max(peak_level(samples));
        if window.0.is_none_or(|t| t.elapsed() >= LEVEL_INTERVAL) {
            self.events.level(window.1);
            *window = (Some(Instant::now()), 0.0);
        }
    }

    fn error(&self, error: AppError) {
        self.events.error(PreviewKind::Microphone, error);
    }
}

#[derive(Default)]
pub struct PreviewSession {
    screen: Option<Box<dyn ScreenCapture>>,
    camera: Option<Box<dyn CameraCapture>>,
    microphone: Option<Box<dyn MicrophoneCapture>>,
}

/// Preview size for a source: small, preserving the aspect ratio.
fn preview_size(source: Size) -> Size {
    let scale = (SCREEN_THUMBNAIL.0 as f64 / f64::from(source.width.max(1)))
        .min(SCREEN_THUMBNAIL.1 as f64 / f64::from(source.height.max(1)))
        .min(1.0);
    let even = |v: f64| ((v.round() as u32).max(2) / 2) * 2;
    Size::new(
        even(f64::from(source.width) * scale),
        even(f64::from(source.height) * scale),
    )
}

impl PreviewSession {
    pub fn start<P: Platform>(request: &PreviewRequest, events: Arc<dyn PreviewEvents>) -> Self {
        let mut session = Self::default();
        let throttled = |throttle: FrameThrottle| {
            Arc::new(ThrottledFrames {
                throttle,
                events: events.clone(),
            })
        };

        if let Some(source) = &request.source {
            let result = P::source_size(source).and_then(|size| {
                let mut screen = P::screen_capture(ScreenCaptureRequest {
                    source: source.clone(),
                    output_size: preview_size(size),
                    fps: 2,
                    show_cursor: false,
                })?;
                screen.start(throttled(FrameThrottle::screen()))?;
                Ok(screen)
            });
            match result {
                Ok(screen) => session.screen = Some(screen),
                Err(error) => events.error(PreviewKind::Screen, error),
            }
        }

        if let Some(id) = &request.camera_id {
            let result = P::camera_capture(id).and_then(|mut camera| {
                camera.start(throttled(FrameThrottle::camera()))?;
                Ok(camera)
            });
            match result {
                Ok(camera) => session.camera = Some(camera),
                Err(error) => events.error(PreviewKind::Camera, error),
            }
        }

        if let Some(id) = &request.microphone_id {
            let meter = Arc::new(LevelMeter {
                window: Mutex::new((None, 0.0)),
                events: events.clone(),
            });
            let result = P::microphone_capture(id).and_then(|mut microphone| {
                microphone.start(meter)?;
                Ok(microphone)
            });
            match result {
                Ok(microphone) => session.microphone = Some(microphone),
                Err(error) => events.error(PreviewKind::Microphone, error),
            }
        }
        session
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.screen.take() {
            c.stop();
        }
        if let Some(mut c) = self.camera.take() {
            c.stop();
        }
        if let Some(mut c) = self.microphone.take() {
            c.stop();
        }
    }
}

impl Drop for PreviewSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_size_fits_and_keeps_aspect_ratio() {
        assert_eq!(preview_size(Size::new(3840, 2160)), Size::new(960, 540));
        assert_eq!(preview_size(Size::new(2880, 1800)), Size::new(864, 540));
        assert_eq!(preview_size(Size::new(640, 480)), Size::new(640, 480));
    }
}
