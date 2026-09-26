//! Records from the command line with the real platform backend.
//!
//! Useful for exercising capture and encoding without the UI:
//!
//! ```sh
//! cargo run --release --example record -- --seconds 5 --camera --microphone
//! # camera as a square centered at 25% / 30% of the frame, 30% of its height:
//! cargo run --release --example record -- --camera --overlay square:0.25:0.3:0.3
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use codetake_lib::config::*;
use codetake_lib::platform::{Current, Platform};
use codetake_lib::recording::recorder::{
    self, RecorderEvents, RecordingOutcome, RecordingStatus, StartRequest,
};

struct PrintEvents;

impl RecorderEvents for PrintEvents {
    fn status(&self, status: &RecordingStatus) {
        eprintln!(
            "{:?} {:>6} ms  mic {:.2}  system {:.2}  {:?}",
            status.state,
            status.elapsed_ms,
            status.microphone_level,
            status.system_audio_level,
            status.warnings
        );
    }
    fn finished(&self, outcome: &RecordingOutcome) {
        eprintln!("finished: {outcome:?}");
    }
}

/// Parses `shape:x:y:size`, e.g. `circle:0.9:0.85:0.25`.
fn parse_overlay(spec: &str) -> CameraOverlay {
    let parts: Vec<&str> = spec.split(':').collect();
    let number = |i: usize| parts.get(i).and_then(|v| v.parse().ok());
    let default = CameraOverlay::default();
    CameraOverlay {
        shape: match parts.first().copied() {
            Some("square") => OverlayShape::Square,
            Some("rectangle") => OverlayShape::Rectangle,
            _ => OverlayShape::Circle,
        },
        x: number(1).unwrap_or(default.x),
        y: number(2).unwrap_or(default.y),
        size: number(3).unwrap_or(default.size),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    println!("capabilities: {:?}", Current::capabilities());
    println!("permissions: {:?}", Current::permissions());
    if flag("--info") {
        println!("displays: {:?}", Current::displays());
        println!("windows: {:?}", Current::windows().map(|w| w.len()));
        println!("cameras: {:?}", Current::cameras());
        println!("microphones: {:?}", Current::microphones());
        return;
    }

    let displays = Current::displays().expect("list displays");
    let display = displays.first().expect("a display");
    let cameras = Current::cameras().unwrap_or_default();
    let microphones = Current::microphones().unwrap_or_default();
    let seconds: f64 = value("--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5.0);
    let output = value("--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("codetake-example"));

    let config = RecordingConfig {
        source: CaptureSource::Display { id: display.id },
        camera: flag("--camera").then(|| CameraConfig {
            device_id: value("--camera-id")
                .or_else(|| cameras.iter().find(|c| c.is_default).map(|c| c.id.clone()))
                .unwrap_or_else(|| cameras.first().expect("a camera").id.clone()),
            overlay: value("--overlay")
                .map_or_else(CameraOverlay::default, |spec| parse_overlay(&spec)),
        }),
        microphone: flag("--microphone").then(|| MicrophoneConfig {
            device_id: microphones
                .iter()
                .find(|m| m.is_default)
                .or(microphones.first())
                .expect("a microphone")
                .id
                .clone(),
        }),
        system_audio: flag("--system-audio"),
        music: value("--music").map(|_| MusicConfig {
            track_id: "example".into(),
            volume: 0.5,
        }),
        resolution: if flag("--1080p") {
            Resolution::P1080
        } else {
            Resolution::Source
        },
        fps: if flag("--60") {
            Fps::Sixty
        } else {
            Fps::Thirty
        },
        output_directory: output,
    };
    let handle = recorder::start::<Current>(
        StartRequest {
            music_path: value("--music").map(PathBuf::from),
            config,
        },
        Arc::new(PrintEvents),
    )
    .expect("start recording");
    std::thread::sleep(Duration::from_secs_f64(seconds));
    let outcome = handle.stop().expect("stop");
    println!("{outcome:?}");
}
