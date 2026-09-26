//! Abstractions over platform capture and encoding.
//!
//! The recorder only talks to these traits. Each platform provides
//! implementations in `crate::platform`, so the recording pipeline, the
//! mixer and the compositor stay platform independent.

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{CaptureSource, Fps, Size};
use crate::error::{AppError, AppResult};
use crate::video::compositor::{BgraMut, BgraRef};

/// A captured video frame in BGRA format.
pub trait VideoFrame: Send + Sync {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    /// Gives `read` access to the pixels for the duration of the call.
    fn read(&self, read: &mut dyn FnMut(BgraRef<'_>));
}

pub type SharedFrame = Arc<dyn VideoFrame>;

/// Receives frames from a screen or camera capture.
///
/// Called on capture threads; implementations must return quickly.
pub trait VideoSink: Send + Sync {
    /// `host_time` is the capture time on the platform's monotonic host clock, in seconds.
    fn frame(&self, frame: SharedFrame, host_time: f64);
    /// The capture stopped unexpectedly.
    fn error(&self, error: AppError);
}

/// Receives audio from a microphone or system audio capture.
///
/// Samples are always 48 kHz interleaved stereo `f32`.
pub trait AudioSink: Send + Sync {
    fn audio(&self, host_time: f64, samples: &[f32]);
    fn error(&self, error: AppError);
}

/// Parameters for starting a screen capture.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenCaptureRequest {
    pub source: CaptureSource,
    /// Frames are delivered at this size (scaled by the OS if needed).
    pub output_size: Size,
    /// Maximum frame rate to deliver.
    pub fps: u32,
    pub show_cursor: bool,
}

pub trait ScreenCapture: Send {
    fn start(&mut self, sink: Arc<dyn VideoSink>) -> AppResult<()>;
    fn stop(&mut self);
}

pub trait CameraCapture: Send {
    fn start(&mut self, sink: Arc<dyn VideoSink>) -> AppResult<()>;
    fn stop(&mut self);
}

pub trait MicrophoneCapture: Send {
    fn start(&mut self, sink: Arc<dyn AudioSink>) -> AppResult<()>;
    fn stop(&mut self);
}

pub trait SystemAudioCapture: Send {
    fn start(&mut self, sink: Arc<dyn AudioSink>) -> AppResult<()>;
    fn stop(&mut self);
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncoderSettings {
    pub path: PathBuf,
    pub size: Size,
    pub fps: Fps,
    pub video_bitrate: u32,
    pub audio: bool,
}

/// Writes H.264/AAC into an MP4 file progressively.
pub trait VideoEncoder: Send {
    /// Appends one video frame at `media_time` seconds. `fill` draws the
    /// frame into an encoder-owned buffer of the configured size.
    ///
    /// Returns `Ok(false)` if the encoder is busy and the frame was dropped.
    fn append_video(
        &mut self,
        media_time: f64,
        fill: &mut dyn FnMut(&mut BgraMut<'_>),
    ) -> AppResult<bool>;

    /// Appends mixed 48 kHz interleaved stereo audio starting at `media_time`.
    fn append_audio(&mut self, media_time: f64, samples: &[f32]) -> AppResult<()>;

    /// Ends the session at `end_time` and finalizes the file.
    fn finish(&mut self, end_time: f64) -> AppResult<()>;
}
