//! Exports a recording with background music from the command line.
//!
//! ```sh
//! cargo run --release --example export -- <recording.mp4> <music.m4a> [music-volume] [recording-volume]
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use codetake_lib::export::{export_path, ExportJob, ExportMusic};
use codetake_lib::platform::{Current, Platform};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = PathBuf::from(args.get(1).expect("recording path"));
    let music = PathBuf::from(args.get(2).expect("music file"));
    let music_volume: f32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.3);
    let recording_volume: f32 = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(1.0);

    let job = ExportJob {
        destination: export_path(&source, |p| p.exists()),
        source,
        recording_volume,
        music: Some(ExportMusic {
            samples: Arc::new(Current::decode_audio_file(&music).expect("decode music")),
            volume: music_volume,
        }),
    };
    let started = Instant::now();
    let mut last = -1.0;
    Current::export_recording(&job, &mut |fraction| {
        if fraction - last >= 0.25 || fraction >= 1.0 {
            eprintln!("{:>3.0}%", fraction * 100.0);
            last = fraction;
        }
    })
    .expect("export");
    println!(
        "exported {} in {:.2?}",
        job.destination.display(),
        started.elapsed()
    );
}
