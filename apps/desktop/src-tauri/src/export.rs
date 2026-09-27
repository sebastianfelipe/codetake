//! Exports the finished video from a raw recording.
//!
//! Recordings are saved raw: the screen (with microphone and system audio)
//! and the webcam in separate files, without music. In the review step the
//! user places the webcam, picks music and balances it against the
//! recording; the export then writes the final file, as cheaply as possible
//! ([`plan_export`]):
//!
//! - webcam shown → composite it onto the screen (video re-encoded);
//! - otherwise, music or a changed voice volume → re-mix only the audio,
//!   copying the video untouched;
//! - otherwise → copy the screen recording.
//!
//! Audio mixing:
//!
//! ```text
//! recording audio × recording volume ─┐
//!                                     ├─▶ soft limit ─▶ AAC
//! music (looped, faded) × music volume┘
//! ```
//!
//! This module holds the platform-independent parts (mixing, fades, file
//! names); `Platform::export_recording` does the reading and writing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::audio::mixer::soft_clip;
use crate::audio::{CHANNELS, SAMPLE_RATE};
use crate::config::{CameraOverlay, Fps};

/// Music fades in over this long at the start of the video...
pub const FADE_IN_SECONDS: f64 = 1.5;
/// ...and out over this long before the end.
pub const FADE_OUT_SECONDS: f64 = 3.0;
/// Recording volume can boost quiet recordings up to this gain.
pub const MAX_RECORDING_VOLUME: f32 = 2.0;

/// What the user chose in the review step.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    /// The raw screen recording.
    pub screen: PathBuf,
    /// The raw webcam recording, if there is one.
    pub camera: Option<PathBuf>,
    /// Where to place the webcam, or `None` to leave it out.
    pub overlay: Option<CameraOverlay>,
    /// Background music, if any.
    pub track_id: Option<String>,
    /// Linear gain for the music, 0..=1.
    pub music_volume: f32,
    /// Linear gain for the recorded audio, 0..=2.
    pub recording_volume: f32,
    /// Where to write the video (a free name next to it is used if taken).
    pub destination: PathBuf,
    /// Frame rate of the recording.
    pub fps: u32,
}

impl ExportSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.music_volume) {
            return Err("music volume must be between 0 and 1".into());
        }
        if !(0.0..=MAX_RECORDING_VOLUME).contains(&self.recording_volume) {
            return Err(format!(
                "recording volume must be between 0 and {MAX_RECORDING_VOLUME}"
            ));
        }
        if self.track_id.as_ref().is_some_and(|t| t.trim().is_empty()) {
            return Err("no music track selected".into());
        }
        if self.overlay.is_some() && self.camera.is_none() {
            return Err("there is no camera recording to show".into());
        }
        if Fps::try_from(self.fps).is_err() {
            return Err(format!("unsupported frame rate {}", self.fps));
        }
        Ok(())
    }

    pub fn plan(&self) -> ExportPlan {
        plan_export(
            self.overlay.is_some() && self.camera.is_some(),
            self.track_id.is_some(),
            self.recording_volume,
        )
    }
}

/// How the final video is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportPlan {
    /// Nothing to change: copy the screen recording.
    Copy,
    /// Re-mix the audio (music, voice volume); copy the video untouched.
    RemixAudio,
    /// Composite the webcam onto the screen; re-encode the video.
    Composite,
}

pub fn plan_export(show_camera: bool, music: bool, recording_volume: f32) -> ExportPlan {
    if show_camera {
        ExportPlan::Composite
    } else if music || (recording_volume - 1.0).abs() > f32::EPSILON {
        ExportPlan::RemixAudio
    } else {
        ExportPlan::Copy
    }
}

/// Background music for an export: decoded samples (48 kHz interleaved
/// stereo) and a volume, looped and faded to the video by [`MusicBed`].
#[derive(Debug, Clone)]
pub struct ExportMusic {
    pub samples: Arc<Vec<f32>>,
    pub volume: f32,
}

/// Re-mixes the audio of a recording, copying its video.
pub struct ExportJob {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub recording_volume: f32,
    pub music: Option<ExportMusic>,
}

/// Composites the webcam onto the screen recording.
pub struct CompositeJob {
    pub screen: PathBuf,
    pub camera: PathBuf,
    pub overlay: CameraOverlay,
    pub destination: PathBuf,
    pub fps: Fps,
    pub recording_volume: f32,
    pub music: Option<ExportMusic>,
}

/// Looping background music with fades, positioned on the video timeline.
#[derive(Debug, Clone)]
pub struct MusicBed {
    /// Interleaved stereo at the mixer sample rate.
    samples: Arc<Vec<f32>>,
    gain: f32,
    /// Length of the video, in frames; the fade-out ends here.
    total_frames: u64,
}

impl MusicBed {
    /// A bed for `music` (or silence) under a video of the given length.
    pub fn for_export(music: Option<&ExportMusic>, duration_seconds: f64) -> Self {
        match music {
            Some(music) => Self::new(music.samples.clone(), music.volume, duration_seconds),
            None => Self::new(Arc::new(Vec::new()), 0.0, duration_seconds),
        }
    }

    pub fn new(samples: Arc<Vec<f32>>, gain: f32, duration_seconds: f64) -> Self {
        Self {
            samples,
            gain,
            total_frames: (duration_seconds.max(0.0) * f64::from(SAMPLE_RATE)).round() as u64,
        }
    }

    /// Fade envelope (0..=1) at `frame`.
    pub fn envelope(&self, frame: u64) -> f32 {
        let rate = f64::from(SAMPLE_RATE);
        let t = frame as f64 / rate;
        let remaining = (self.total_frames.saturating_sub(frame)) as f64 / rate;
        let fade_in = (t / FADE_IN_SECONDS).min(1.0);
        let fade_out = (remaining / FADE_OUT_SECONDS).min(1.0);
        (fade_in.min(fade_out)) as f32
    }

    /// Adds the music for `[start_frame, start_frame + frames)` into `out`.
    pub fn add_to(&self, out: &mut [f32], start_frame: u64) {
        let track_frames = (self.samples.len() / CHANNELS) as u64;
        if track_frames == 0 || self.gain == 0.0 {
            return;
        }
        for (i, frame) in out.chunks_exact_mut(CHANNELS).enumerate() {
            let position = start_frame + i as u64;
            let gain = self.gain * self.envelope(position);
            let source = ((position % track_frames) as usize) * CHANNELS;
            for (c, sample) in frame.iter_mut().enumerate() {
                *sample += self.samples[source + c] * gain;
            }
        }
    }
}

/// Mixes one block of recorded audio (48 kHz interleaved stereo) starting at
/// `start_frame` with the music, returning the limited result.
pub fn mix_block(
    recording: &[f32],
    recording_volume: f32,
    music: &MusicBed,
    start_frame: u64,
) -> Vec<f32> {
    let mut out: Vec<f32> = recording.iter().map(|s| s * recording_volume).collect();
    music.add_to(&mut out, start_frame);
    for sample in &mut out {
        *sample = soft_clip(*sample);
    }
    out
}

/// Music only, for recordings without an audio track.
pub fn music_block(frames: usize, music: &MusicBed, start_frame: u64) -> Vec<f32> {
    mix_block(&vec![0.0; frames * CHANNELS], 0.0, music, start_frame)
}

/// `coding-session-….mp4` → `coding-session-…-with-music.mp4`, adding a
/// counter if that name is taken.
pub fn export_path(source: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "recording".into());
    let mut counter = 1;
    loop {
        let name = if counter == 1 {
            format!("{stem}-with-music.mp4")
        } else {
            format!("{stem}-with-music-{counter}.mp4")
        };
        let candidate = source.with_file_name(name);
        if !exists(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Progress reported to the UI while exporting.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// 0..=1
    pub fraction: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u64 = SAMPLE_RATE as u64;

    fn bed(value: f32, frames: usize, gain: f32, seconds: f64) -> MusicBed {
        MusicBed::new(Arc::new(vec![value; frames * CHANNELS]), gain, seconds)
    }

    #[test]
    fn music_fades_in_and_out() {
        let music = bed(1.0, 100, 1.0, 20.0);
        assert_eq!(music.envelope(0), 0.0);
        assert!((music.envelope((0.75 * RATE as f64) as u64) - 0.5).abs() < 1e-3);
        assert_eq!(music.envelope(5 * RATE), 1.0);
        assert!((music.envelope(RATE * 20 - (1.5 * RATE as f64) as u64) - 0.5).abs() < 1e-3);
        assert_eq!(music.envelope(20 * RATE), 0.0);
        assert_eq!(music.envelope(25 * RATE), 0.0);
    }

    #[test]
    fn short_videos_never_reach_full_music_volume_abruptly() {
        // A 2-second video: fade-in and fade-out overlap; the envelope stays smooth.
        let music = bed(1.0, 100, 1.0, 2.0);
        let peak = (0..2 * RATE).map(|f| music.envelope(f)).fold(0.0, f32::max);
        assert!(peak < 1.0 && peak > 0.4);
    }

    #[test]
    fn music_loops_seamlessly() {
        let track: Vec<f32> = (0..4).flat_map(|i| [i as f32 * 0.1; 2]).collect();
        let music = MusicBed::new(Arc::new(track), 1.0, 1000.0);
        let mut out = vec![0.0; 6 * CHANNELS];
        music.add_to(&mut out, 10 * RATE); // well past the fade-in
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let expected = [0.0, 0.1, 0.2, 0.3, 0.0, 0.1];
        let start = (10 * RATE % 4) as usize;
        for (i, value) in left.iter().enumerate() {
            assert!((value - expected[(start + i) % 4]).abs() < 1e-6);
        }
    }

    #[test]
    fn mixes_recording_and_music_with_their_volumes() {
        let music = bed(0.5, 1000, 0.2, 100.0);
        let recording = vec![0.4; 10 * CHANNELS];
        let out = mix_block(&recording, 1.5, &music, 10 * RATE);
        // 0.4 × 1.5 + 0.5 × 0.2 = 0.7
        assert!(out.iter().all(|s| (s - 0.7).abs() < 1e-6));
    }

    #[test]
    fn mixing_never_clips() {
        let music = bed(1.0, 1000, 1.0, 100.0);
        let out = mix_block(&vec![1.0; 100], 2.0, &music, 10 * RATE);
        assert!(out.iter().all(|s| *s < 1.0));
    }

    #[test]
    fn music_only_blocks_contain_just_the_music() {
        let music = bed(0.5, 1000, 0.5, 100.0);
        let out = music_block(8, &music, 10 * RATE);
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn export_names_do_not_overwrite() {
        let source = Path::new("/rec/2026-09-26/coding-session-2026-09-26-09-32-14.mp4");
        let first = export_path(source, |_| false);
        assert_eq!(
            first,
            PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14-with-music.mp4")
        );
        let second = export_path(source, |p| p == first);
        assert!(second.to_string_lossy().ends_with("-with-music-2.mp4"));
    }

    fn settings() -> ExportSettings {
        ExportSettings {
            screen: "/r/raw/a-screen.mp4".into(),
            camera: Some("/r/raw/a-camera.mp4".into()),
            overlay: Some(CameraOverlay::default()),
            track_id: Some("coding-01".into()),
            music_volume: 0.3,
            recording_volume: 1.2,
            destination: "/r/a.mp4".into(),
            fps: 30,
        }
    }

    #[test]
    fn validates_settings() {
        assert!(settings().validate().is_ok());
        assert!(ExportSettings {
            music_volume: 1.5,
            ..settings()
        }
        .validate()
        .is_err());
        assert!(ExportSettings {
            recording_volume: 3.0,
            ..settings()
        }
        .validate()
        .is_err());
        assert!(ExportSettings {
            track_id: Some(" ".into()),
            ..settings()
        }
        .validate()
        .is_err());
        assert!(ExportSettings {
            camera: None,
            ..settings()
        }
        .validate()
        .is_err());
        assert!(ExportSettings {
            fps: 24,
            ..settings()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn chooses_the_cheapest_way_to_export() {
        assert_eq!(settings().plan(), ExportPlan::Composite);
        let no_camera = ExportSettings {
            overlay: None,
            ..settings()
        };
        assert_eq!(no_camera.plan(), ExportPlan::RemixAudio);
        let untouched = ExportSettings {
            overlay: None,
            track_id: None,
            recording_volume: 1.0,
            ..settings()
        };
        assert_eq!(untouched.plan(), ExportPlan::Copy);
        let louder = ExportSettings {
            recording_volume: 1.5,
            ..untouched
        };
        assert_eq!(louder.plan(), ExportPlan::RemixAudio);
    }

    #[test]
    fn silent_music_bed_leaves_the_recording_alone() {
        let bed = MusicBed::for_export(None, 10.0);
        let out = mix_block(&[0.3; 8], 1.0, &bed, 5 * RATE);
        assert!(out.iter().all(|s| (s - 0.3).abs() < 1e-6));
    }
}
