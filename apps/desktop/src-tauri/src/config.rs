//! The user's recording configuration and the rules that validate it.
//!
//! The frontend sends a [`RecordingConfig`] as JSON when the user presses
//! Record. Everything here is platform independent and unit tested.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// What part of the screen is captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaptureSource {
    Display { id: u32 },
    Window { id: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayShape {
    Circle,
    /// A square with slightly rounded corners.
    Square,
    /// A 4:3 rectangle with rounded corners.
    Rectangle,
}

/// Smallest and largest overlay height, as a fraction of the video height.
pub const OVERLAY_MIN_SIZE: f64 = 0.08;
pub const OVERLAY_MAX_SIZE: f64 = 0.6;

/// Where the webcam goes and how big it is. Positions and sizes are
/// relative to the video frame, so they apply at any output resolution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraOverlay {
    pub shape: OverlayShape,
    /// Overlay height as a fraction of the video height.
    pub size: f64,
    /// Horizontal center of the overlay, from 0 (left edge) to 1 (right edge).
    pub x: f64,
    /// Vertical center of the overlay, from 0 (top edge) to 1 (bottom edge).
    pub y: f64,
}

impl Default for CameraOverlay {
    /// A medium circle in the bottom-right corner.
    fn default() -> Self {
        Self {
            shape: OverlayShape::Circle,
            size: 0.25,
            x: 0.91,
            y: 0.84,
        }
    }
}

impl CameraOverlay {
    fn validate(&self) -> AppResult<()> {
        let in_range = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !in_range(self.size, OVERLAY_MIN_SIZE, OVERLAY_MAX_SIZE) {
            return Err(AppError::InvalidConfig(format!(
                "camera size must be between {OVERLAY_MIN_SIZE} and {OVERLAY_MAX_SIZE}"
            )));
        }
        if !in_range(self.x, 0.0, 1.0) || !in_range(self.y, 0.0, 1.0) {
            return Err(AppError::InvalidConfig(
                "camera position must be inside the frame".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraConfig {
    pub device_id: String,
    #[serde(default)]
    pub overlay: CameraOverlay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneConfig {
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicConfig {
    pub track_id: String,
    /// Linear gain between 0.0 and 1.0.
    pub volume: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    /// Keep the captured source's native pixel size.
    Source,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "1440p")]
    P1440,
    #[serde(rename = "2160p")]
    P2160,
}

impl Resolution {
    fn target_height(self) -> Option<u32> {
        match self {
            Resolution::Source => None,
            Resolution::P1080 => Some(1080),
            Resolution::P1440 => Some(1440),
            Resolution::P2160 => Some(2160),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub enum Fps {
    Thirty,
    Sixty,
}

impl Fps {
    pub fn as_u32(self) -> u32 {
        match self {
            Fps::Thirty => 30,
            Fps::Sixty => 60,
        }
    }
}

impl TryFrom<u32> for Fps {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            30 => Ok(Fps::Thirty),
            60 => Ok(Fps::Sixty),
            other => Err(format!("unsupported frame rate {other}; use 30 or 60")),
        }
    }
}

impl From<Fps> for u32 {
    fn from(value: Fps) -> Self {
        value.as_u32()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingConfig {
    pub source: CaptureSource,
    pub camera: Option<CameraConfig>,
    pub microphone: Option<MicrophoneConfig>,
    #[serde(default)]
    pub system_audio: bool,
    pub music: Option<MusicConfig>,
    pub resolution: Resolution,
    pub fps: Fps,
    pub output_directory: PathBuf,
}

impl RecordingConfig {
    pub fn from_json(json: &str) -> AppResult<Self> {
        let config: RecordingConfig =
            serde_json::from_str(json).map_err(|e| AppError::InvalidConfig(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Checks invariants serde cannot express.
    pub fn validate(&self) -> AppResult<()> {
        if let Some(camera) = &self.camera {
            if camera.device_id.trim().is_empty() {
                return Err(AppError::InvalidConfig("no camera selected".into()));
            }
            camera.overlay.validate()?;
        }
        if let Some(microphone) = &self.microphone {
            if microphone.device_id.trim().is_empty() {
                return Err(AppError::InvalidConfig("no microphone selected".into()));
            }
        }
        if let Some(music) = &self.music {
            if music.track_id.trim().is_empty() {
                return Err(AppError::InvalidConfig("no music track selected".into()));
            }
            if !(0.0..=1.0).contains(&music.volume) || music.volume.is_nan() {
                return Err(AppError::InvalidConfig(
                    "music volume must be between 0 and 1".into(),
                ));
            }
        }
        if self.output_directory.as_os_str().is_empty() {
            return Err(AppError::InvalidConfig(
                "no output directory selected".into(),
            ));
        }
        if !self.output_directory.is_absolute() {
            return Err(AppError::InvalidConfig(
                "the output directory must be an absolute path".into(),
            ));
        }
        Ok(())
    }

    pub fn has_audio(&self) -> bool {
        self.microphone.is_some() || self.system_audio || self.music.is_some()
    }
}

/// Pixel dimensions of a video frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// The largest frame the H.264 encoder accepts (level 5.2: 36,864 macroblocks,
/// at most 4096 pixels on either side).
const H264_MAX_MACROBLOCKS: u64 = 36_864;
const H264_MAX_DIMENSION: u32 = 4096;

fn round_down_to_even(value: f64) -> u32 {
    let value = value.floor().max(2.0) as u32;
    value - value % 2
}

/// Computes the encoded frame size for a captured source.
///
/// The aspect ratio is preserved, frames are never upscaled, dimensions are
/// even (required by 4:2:0 chroma subsampling), and the result is clamped to
/// what the H.264 encoder can handle.
pub fn output_size(source: Size, resolution: Resolution) -> Size {
    let source_w = f64::from(source.width.max(2));
    let source_h = f64::from(source.height.max(2));

    let mut scale = match resolution.target_height() {
        Some(target) if f64::from(target) < source_h => f64::from(target) / source_h,
        _ => 1.0,
    };

    let max_dimension = f64::from(H264_MAX_DIMENSION);
    scale = scale
        .min(max_dimension / source_w)
        .min(max_dimension / source_h);

    let macroblocks = (source_w * scale / 16.0).ceil() * (source_h * scale / 16.0).ceil();
    let max_macroblocks = H264_MAX_MACROBLOCKS as f64;
    if macroblocks > max_macroblocks {
        scale *= (max_macroblocks / macroblocks).sqrt() * 0.995;
    }

    Size::new(
        round_down_to_even(source_w * scale),
        round_down_to_even(source_h * scale),
    )
}

/// Whether choosing `resolution` would produce a different size than the
/// source (i.e. whether the option is meaningful for this source).
pub fn resolution_is_available(source: Size, resolution: Resolution) -> bool {
    match resolution.target_height() {
        None => true,
        Some(target) => source.height >= target,
    }
}

/// Average H.264 bitrate for screen content, in bits per second.
///
/// Screen recordings of code contain sharp text, so we use a generous
/// bits-per-pixel budget. Higher frame rates need proportionally fewer bits
/// per frame because consecutive frames are more similar.
pub fn video_bitrate(size: Size, fps: Fps) -> u32 {
    let pixels = f64::from(size.width) * f64::from(size.height);
    let bits_per_pixel = match fps {
        Fps::Thirty => 0.12,
        Fps::Sixty => 0.09,
    };
    let bitrate = pixels * f64::from(fps.as_u32()) * bits_per_pixel;
    bitrate.clamp(2_000_000.0, 80_000_000.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_json() -> String {
        r#"{
            "source": { "kind": "display", "id": 1 },
            "camera": {
                "deviceId": "cam-1",
                "overlay": { "shape": "square", "size": 0.3, "x": 0.2, "y": 0.75 }
            },
            "microphone": { "deviceId": "mic-1" },
            "systemAudio": true,
            "music": { "trackId": "coding-01", "volume": 0.3 },
            "resolution": "1080p",
            "fps": 60,
            "outputDirectory": "/Users/dev/Movies/CodeTake"
        }"#
        .to_string()
    }

    #[test]
    fn parses_a_full_configuration() {
        let config = RecordingConfig::from_json(&sample_json()).unwrap();
        assert_eq!(config.source, CaptureSource::Display { id: 1 });
        assert_eq!(config.resolution, Resolution::P1080);
        assert_eq!(config.fps, Fps::Sixty);
        assert!(config.system_audio);
        let camera = config.camera.unwrap();
        assert_eq!(camera.overlay.shape, OverlayShape::Square);
        assert_eq!((camera.overlay.x, camera.overlay.y), (0.2, 0.75));
        assert_eq!(config.music.unwrap().track_id, "coding-01");
    }

    #[test]
    fn parses_a_minimal_configuration() {
        let json = r#"{
            "source": { "kind": "window", "id": 42 },
            "camera": null,
            "microphone": null,
            "music": null,
            "resolution": "source",
            "fps": 30,
            "outputDirectory": "/tmp/out"
        }"#;
        let config = RecordingConfig::from_json(json).unwrap();
        assert_eq!(config.source, CaptureSource::Window { id: 42 });
        assert!(!config.system_audio);
        assert!(!config.has_audio());
    }

    #[test]
    fn camera_overlay_defaults_to_bottom_right_circle() {
        let json = sample_json().replace(
            r#""overlay": { "shape": "square", "size": 0.3, "x": 0.2, "y": 0.75 }"#,
            r#""unused": true"#,
        );
        let config = RecordingConfig::from_json(&json).unwrap();
        assert_eq!(config.camera.unwrap().overlay, CameraOverlay::default());
    }

    #[test]
    fn rejects_overlays_outside_the_frame_or_out_of_size_range() {
        for overlay in [
            r#""size": 0.3, "x": 1.2, "y": 0.5"#,
            r#""size": 0.3, "x": 0.5, "y": -0.1"#,
            r#""size": 0.9, "x": 0.5, "y": 0.5"#,
            r#""size": 0.01, "x": 0.5, "y": 0.5"#,
        ] {
            let json = sample_json().replace(r#""size": 0.3, "x": 0.2, "y": 0.75"#, overlay);
            assert!(
                matches!(
                    RecordingConfig::from_json(&json),
                    Err(AppError::InvalidConfig(_))
                ),
                "{overlay}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_frame_rates() {
        let json = sample_json().replace(r#""fps": 60"#, r#""fps": 24"#);
        let error = RecordingConfig::from_json(&json).unwrap_err();
        assert!(error.to_string().contains("unsupported frame rate 24"));
    }

    #[test]
    fn rejects_out_of_range_music_volume() {
        let json = sample_json().replace(r#""volume": 0.3"#, r#""volume": 1.5"#);
        assert!(matches!(
            RecordingConfig::from_json(&json),
            Err(AppError::InvalidConfig(_))
        ));
    }

    #[test]
    fn rejects_empty_device_ids() {
        let json = sample_json().replace(r#""deviceId": "mic-1""#, r#""deviceId": " ""#);
        assert!(RecordingConfig::from_json(&json).is_err());
    }

    #[test]
    fn rejects_relative_output_directories() {
        let json = sample_json().replace("/Users/dev/Movies/CodeTake", "Movies/CodeTake");
        assert!(RecordingConfig::from_json(&json).is_err());
    }

    #[test]
    fn source_resolution_keeps_native_size() {
        let size = output_size(Size::new(2880, 1800), Resolution::Source);
        assert_eq!(size, Size::new(2880, 1800));
    }

    #[test]
    fn downscales_preserving_aspect_ratio() {
        assert_eq!(
            output_size(Size::new(3840, 2160), Resolution::P1080),
            Size::new(1920, 1080)
        );
        assert_eq!(
            output_size(Size::new(2880, 1800), Resolution::P1080),
            Size::new(1728, 1080)
        );
    }

    #[test]
    fn never_upscales() {
        assert_eq!(
            output_size(Size::new(1920, 1080), Resolution::P2160),
            Size::new(1920, 1080)
        );
    }

    #[test]
    fn produces_even_dimensions() {
        let size = output_size(Size::new(1513, 983), Resolution::Source);
        assert_eq!(size.width % 2, 0);
        assert_eq!(size.height % 2, 0);
    }

    #[test]
    fn clamps_oversized_sources_to_encoder_limits() {
        // A 6K display cannot be encoded at native size with H.264.
        let size = output_size(Size::new(6016, 3384), Resolution::Source);
        assert!(size.width <= 4096 && size.height <= 4096);
        let macroblocks = u64::from(size.width.div_ceil(16)) * u64::from(size.height.div_ceil(16));
        assert!(macroblocks <= H264_MAX_MACROBLOCKS);
        let aspect = f64::from(size.width) / f64::from(size.height);
        assert!((aspect - 6016.0 / 3384.0).abs() < 0.01);
    }

    #[test]
    fn resolution_availability_depends_on_source() {
        let source = Size::new(2560, 1440);
        assert!(resolution_is_available(source, Resolution::Source));
        assert!(resolution_is_available(source, Resolution::P1440));
        assert!(!resolution_is_available(source, Resolution::P2160));
    }

    #[test]
    fn bitrate_scales_with_pixels_and_is_bounded() {
        let hd = video_bitrate(Size::new(1920, 1080), Fps::Thirty);
        let uhd = video_bitrate(Size::new(3840, 2160), Fps::Thirty);
        assert!(uhd > hd * 3);
        assert_eq!(video_bitrate(Size::new(64, 64), Fps::Thirty), 2_000_000);
        assert!(video_bitrate(Size::new(4096, 2304), Fps::Sixty) <= 80_000_000);
    }
}
