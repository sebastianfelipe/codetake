//! The explicit model of one recording.
//!
//! A `RecordingSession` captures everything decided when recording starts:
//! what is recorded, how it is encoded and where it is written. It is kept
//! separate from the running capture machinery so that later features (for
//! example an editor working on finished sessions) can build on it.

use chrono::{DateTime, Local};
use serde::Serialize;
use std::path::PathBuf;

use crate::config::{CaptureSource, Fps, RecordingConfig, Resolution, Size};
use crate::output::RecordingFiles;

#[derive(Debug, Clone)]
pub struct RecordingSession {
    pub config: RecordingConfig,
    pub source_size: Size,
    pub output_size: Size,
    pub video_bitrate: u32,
    pub started_at: DateTime<Local>,
    pub output: RecordingFiles,
}

/// What the UI needs to know about a session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub source: CaptureSource,
    pub camera: bool,
    pub microphone: bool,
    pub system_audio: bool,
    pub music: Option<String>,
    pub resolution: Resolution,
    pub fps: Fps,
    pub output_size: Size,
    pub started_at: String,
    pub output_path: PathBuf,
}

impl RecordingSession {
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            source: self.config.source.clone(),
            camera: self.config.camera.is_some(),
            microphone: self.config.microphone.is_some(),
            system_audio: self.config.system_audio,
            music: self.config.music.as_ref().map(|m| m.track_id.clone()),
            resolution: self.config.resolution,
            fps: self.config.fps,
            output_size: self.output_size,
            started_at: self.started_at.to_rfc3339(),
            output_path: self.output.export_path.clone(),
        }
    }
}
