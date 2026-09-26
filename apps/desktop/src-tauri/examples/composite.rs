//! Composites a raw camera recording onto a raw screen recording.
//!
//! ```sh
//! cargo run --release --example composite -- <screen.mp4> <camera.mp4> <out.mp4> \
//!   [shape:x:y:size] [music.m4a]
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use codetake_lib::config::{CameraOverlay, Fps, OverlayShape};
use codetake_lib::export::{CompositeJob, ExportMusic};
use codetake_lib::platform::{Current, Platform};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args.get(i).cloned();
    let overlay = arg(4).map_or_else(CameraOverlay::default, |spec| {
        let parts: Vec<&str> = spec.split(':').collect();
        let number = |i: usize| parts.get(i).and_then(|v| v.parse().ok());
        CameraOverlay {
            shape: match parts.first().copied() {
                Some("square") => OverlayShape::Square,
                Some("rectangle") => OverlayShape::Rectangle,
                _ => OverlayShape::Circle,
            },
            x: number(1).unwrap_or(0.91),
            y: number(2).unwrap_or(0.84),
            size: number(3).unwrap_or(0.25),
        }
    });
    let job = CompositeJob {
        screen: PathBuf::from(arg(1).expect("screen recording")),
        camera: PathBuf::from(arg(2).expect("camera recording")),
        destination: PathBuf::from(arg(3).expect("output path")),
        overlay,
        fps: Fps::Thirty,
        recording_volume: 1.0,
        music: arg(5).map(|path| ExportMusic {
            samples: Arc::new(Current::decode_audio_file(path.as_ref()).expect("decode music")),
            volume: 0.3,
        }),
    };
    let started = Instant::now();
    Current::composite_recording(&job, &mut |_| {}).expect("composite");
    println!("wrote {} in {:.2?}", job.destination.display(), started.elapsed());
}
