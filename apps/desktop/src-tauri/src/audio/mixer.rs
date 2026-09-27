//! Mixes microphone, system audio and background music into one track.
//!
//! Captured sources are positioned on the output timeline by their media
//! timestamps. The recorder periodically asks the mixer for everything up to
//! "now minus a small latency window"; sources that have not delivered data
//! for that range contribute silence, so one stalled device can never block
//! the recording. Background music has no timestamps: it simply loops from
//! the start of the recording.

use std::collections::VecDeque;
use std::sync::Arc;

use super::{peak_level, CHANNELS, SAMPLE_RATE};

/// Timestamp jitter tolerated before a source is re-aligned to its timestamps.
const RESYNC_THRESHOLD_FRAMES: i64 = (SAMPLE_RATE / 25) as i64; // 40 ms

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Microphone,
    SystemAudio,
}

impl SourceKind {
    fn index(self) -> usize {
        match self {
            SourceKind::Microphone => 0,
            SourceKind::SystemAudio => 1,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MixerConfig {
    pub microphone_gain: Option<f32>,
    pub system_audio_gain: Option<f32>,
    pub music: Option<MusicTrack>,
}

/// Decoded background music: interleaved stereo at the mixer sample rate.
#[derive(Debug, Clone)]
pub struct MusicTrack {
    pub samples: Arc<Vec<f32>>,
    pub gain: f32,
}

#[derive(Debug, Default)]
struct TimedSource {
    gain: f32,
    /// Interleaved samples starting at output frame `start_frame`.
    buffer: VecDeque<f32>,
    start_frame: Option<i64>,
    peak: f32,
}

impl TimedSource {
    fn new(gain: f32) -> Self {
        Self {
            gain,
            ..Self::default()
        }
    }

    fn end_frame(&self) -> Option<i64> {
        self.start_frame
            .map(|start| start + (self.buffer.len() / CHANNELS) as i64)
    }

    fn push(&mut self, frame: i64, samples: &[f32], emitted: i64) {
        self.peak = self.peak.max(peak_level(samples));
        let mut frame = frame;
        let mut samples = samples;

        if let Some(end) = self.end_frame() {
            let drift = frame - end;
            if drift.abs() <= RESYNC_THRESHOLD_FRAMES {
                // Small jitter: keep the stream contiguous.
                frame = end;
            } else if drift > 0 {
                // A real gap (e.g. dropped buffers): pad it with silence.
                self.buffer
                    .extend(std::iter::repeat_n(0.0, drift as usize * CHANNELS));
            } else {
                // Overlap: skip the part we already have.
                let skip = ((-drift) as usize * CHANNELS).min(samples.len());
                samples = &samples[skip..];
                frame = end;
            }
        } else {
            self.start_frame = Some(frame);
        }

        // Drop anything that falls before audio that was already written.
        if frame < emitted {
            let skip = ((emitted - frame) as usize * CHANNELS).min(samples.len());
            samples = &samples[skip..];
            frame = emitted;
            if self.buffer.is_empty() {
                self.start_frame = Some(frame);
            }
        }
        if self.buffer.is_empty() {
            self.start_frame = Some(frame);
        }
        self.buffer.extend(samples.iter().copied());
    }

    /// Adds this source's samples for `[from, from + frames)` into `out`,
    /// then discards them.
    fn mix_into(&mut self, out: &mut [f32], from: i64) {
        let frames = (out.len() / CHANNELS) as i64;
        let Some(start) = self.start_frame else {
            return;
        };
        let to = from + frames;

        // Discard data before the range (should not normally happen).
        if start < from {
            let drop = (((from - start) as usize) * CHANNELS).min(self.buffer.len());
            self.buffer.drain(..drop);
            self.start_frame = Some(from);
        }
        let start = self.start_frame.unwrap_or(from);
        if start >= to {
            return;
        }
        let offset = ((start - from) as usize) * CHANNELS;
        let available = self.buffer.len().min(out.len() - offset);
        for (i, sample) in self.buffer.drain(..available).enumerate() {
            out[offset + i] += sample * self.gain;
        }
        self.start_frame = Some(start + (available / CHANNELS) as i64);
    }
}

/// Audio produced by the mixer, positioned on the output timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MixedBlock {
    pub start_frame: i64,
    pub samples: Vec<f32>,
}

impl MixedBlock {
    pub fn start_time(&self) -> f64 {
        self.start_frame as f64 / f64::from(SAMPLE_RATE)
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Levels {
    pub microphone: f32,
    pub system_audio: f32,
}

#[derive(Debug)]
pub struct AudioMixer {
    sources: [Option<TimedSource>; 2],
    music: Option<MusicTrack>,
    music_position: usize,
    emitted_frames: i64,
}

impl AudioMixer {
    pub fn new(config: MixerConfig) -> Self {
        Self {
            sources: [
                config.microphone_gain.map(TimedSource::new),
                config.system_audio_gain.map(TimedSource::new),
            ],
            music: config.music.filter(|m| !m.samples.is_empty()),
            music_position: 0,
            emitted_frames: 0,
        }
    }

    /// Adds captured audio (48 kHz interleaved stereo) that starts at
    /// `media_time` seconds on the output timeline.
    pub fn push(&mut self, kind: SourceKind, media_time: f64, samples: &[f32]) {
        let emitted = self.emitted_frames;
        if let Some(source) = self.sources[kind.index()].as_mut() {
            let frame = (media_time * f64::from(SAMPLE_RATE)).round() as i64;
            source.push(frame, samples, emitted);
        }
    }

    /// Mixes everything up to `media_time`. Returns `None` if there is
    /// nothing new to emit.
    pub fn mix_until(&mut self, media_time: f64) -> Option<MixedBlock> {
        // The epsilon guards against `n / RATE * RATE` landing just below `n`.
        let end = (media_time * f64::from(SAMPLE_RATE) + 1e-6).floor() as i64;
        if end <= self.emitted_frames {
            return None;
        }
        let from = self.emitted_frames;
        let frames = (end - from) as usize;
        let mut out = vec![0.0_f32; frames * CHANNELS];

        for source in self.sources.iter_mut().flatten() {
            source.mix_into(&mut out, from);
        }
        self.mix_music(&mut out);

        for sample in &mut out {
            *sample = soft_clip(*sample);
        }
        self.emitted_frames = end;
        Some(MixedBlock {
            start_frame: from,
            samples: out,
        })
    }

    fn mix_music(&mut self, out: &mut [f32]) {
        let Some(music) = &self.music else {
            return;
        };
        let track = &music.samples;
        for (i, sample) in out.iter_mut().enumerate() {
            let index = (self.music_position + i) % track.len();
            *sample += track[index] * music.gain;
        }
        self.music_position = (self.music_position + out.len()) % track.len();
    }

    /// Peak levels since the last call.
    pub fn take_levels(&mut self) -> Levels {
        let mut take = |kind: SourceKind| {
            self.sources[kind.index()]
                .as_mut()
                .map_or(0.0, |s| std::mem::take(&mut s.peak))
        };
        Levels {
            microphone: take(SourceKind::Microphone),
            system_audio: take(SourceKind::SystemAudio),
        }
    }
}

/// Keeps quiet signals untouched and smoothly limits peaks below full scale,
/// so summing several sources never produces harsh digital clipping.
pub fn soft_clip(sample: f32) -> f32 {
    const KNEE: f32 = 0.8;
    const CEILING: f32 = 0.98;
    let magnitude = sample.abs();
    if magnitude <= KNEE {
        sample
    } else {
        let compressed = KNEE + (CEILING - KNEE) * ((magnitude - KNEE) / (CEILING - KNEE)).tanh();
        compressed.copysign(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = SAMPLE_RATE as f64;

    fn stereo(frames: usize, value: f32) -> Vec<f32> {
        vec![value; frames * CHANNELS]
    }

    fn mixer(mic: bool, system: bool) -> AudioMixer {
        AudioMixer::new(MixerConfig {
            microphone_gain: mic.then_some(1.0),
            system_audio_gain: system.then_some(1.0),
            music: None,
        })
    }

    #[test]
    fn emits_silence_when_no_source_delivered_audio() {
        let mut mixer = mixer(true, false);
        let block = mixer.mix_until(0.01).unwrap();
        assert_eq!(block.start_frame, 0);
        assert_eq!(block.frames(), 480);
        assert!(block.samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn blocks_are_contiguous() {
        let mut mixer = mixer(true, false);
        let a = mixer.mix_until(0.1).unwrap();
        let b = mixer.mix_until(0.25).unwrap();
        assert_eq!(b.start_frame, a.start_frame + a.frames() as i64);
        assert!(mixer.mix_until(0.25).is_none());
        assert!((b.start_time() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn sums_sources_at_their_timestamps() {
        let mut mixer = mixer(true, true);
        mixer.push(SourceKind::Microphone, 0.0, &stereo(480, 0.25));
        mixer.push(SourceKind::SystemAudio, 0.005, &stereo(240, 0.25));
        let block = mixer.mix_until(0.01).unwrap();
        assert_eq!(block.samples[0], 0.25);
        assert_eq!(block.samples[239 * CHANNELS], 0.25);
        assert_eq!(block.samples[240 * CHANNELS], 0.5);
        assert_eq!(block.samples[479 * CHANNELS + 1], 0.5);
    }

    #[test]
    fn applies_source_gain() {
        let mut mixer = AudioMixer::new(MixerConfig {
            microphone_gain: Some(0.5),
            ..MixerConfig::default()
        });
        mixer.push(SourceKind::Microphone, 0.0, &stereo(480, 0.4));
        assert_eq!(mixer.mix_until(0.01).unwrap().samples[0], 0.2);
    }

    #[test]
    fn ignores_disabled_sources() {
        let mut mixer = mixer(true, false);
        mixer.push(SourceKind::SystemAudio, 0.0, &stereo(480, 0.5));
        assert!(mixer
            .mix_until(0.01)
            .unwrap()
            .samples
            .iter()
            .all(|&s| s == 0.0));
    }

    #[test]
    fn keeps_jittery_timestamps_contiguous() {
        let mut mixer = mixer(true, false);
        mixer.push(SourceKind::Microphone, 0.0, &stereo(480, 0.1));
        // 5 ms late: within tolerance, appended right after the first buffer.
        mixer.push(SourceKind::Microphone, 0.015, &stereo(480, 0.2));
        let block = mixer.mix_until(0.02).unwrap();
        assert_eq!(block.samples[479 * CHANNELS], 0.1);
        assert_eq!(block.samples[480 * CHANNELS], 0.2);
        assert_eq!(block.samples[959 * CHANNELS], 0.2);
    }

    #[test]
    fn fills_real_gaps_with_silence() {
        let mut mixer = mixer(true, false);
        mixer.push(SourceKind::Microphone, 0.0, &stereo(480, 0.1));
        mixer.push(SourceKind::Microphone, 0.1, &stereo(480, 0.2));
        let block = mixer.mix_until(0.11).unwrap();
        assert_eq!(block.samples[100 * CHANNELS], 0.1);
        assert_eq!(block.samples[2000 * CHANNELS], 0.0);
        assert_eq!(block.samples[(0.1 * RATE) as usize * CHANNELS], 0.2);
    }

    #[test]
    fn drops_audio_that_arrives_after_its_time_was_written() {
        let mut mixer = mixer(true, false);
        mixer.mix_until(0.1).unwrap();
        // Arrives late: covers 0.05..0.15, only 0.1..0.15 can still be used.
        mixer.push(SourceKind::Microphone, 0.05, &stereo(4800, 0.3));
        let block = mixer.mix_until(0.15).unwrap();
        assert_eq!(block.start_frame, 4800);
        assert!(block.samples.iter().all(|&s| s == 0.3));
    }

    #[test]
    fn music_loops_from_the_start() {
        let track: Vec<f32> = (0..4).flat_map(|i| [i as f32 * 0.1; 2]).collect();
        let mut mixer = AudioMixer::new(MixerConfig {
            music: Some(MusicTrack {
                samples: Arc::new(track),
                gain: 1.0,
            }),
            ..MixerConfig::default()
        });
        let block = mixer.mix_until(6.0 / RATE).unwrap();
        let left: Vec<f32> = block.samples.iter().step_by(2).copied().collect();
        let expected = [0.0, 0.1, 0.2, 0.3, 0.0, 0.1];
        for (a, b) in left.iter().zip(expected) {
            assert!((a - b).abs() < 1e-6);
        }
        // The loop continues in the next block.
        let next = mixer.mix_until(7.0 / RATE).unwrap();
        assert!((next.samples[0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn soft_clip_limits_peaks_and_preserves_quiet_signals() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert_eq!(soft_clip(-0.8), -0.8);
        assert!(soft_clip(3.0) < 1.0);
        assert!(soft_clip(-3.0) > -1.0);
        assert!(soft_clip(0.9) > 0.8 && soft_clip(0.9) < 0.9);
    }

    #[test]
    fn mixed_output_never_exceeds_full_scale() {
        let mut mixer = mixer(true, true);
        mixer.push(SourceKind::Microphone, 0.0, &stereo(480, 0.9));
        mixer.push(SourceKind::SystemAudio, 0.0, &stereo(480, 0.9));
        let block = mixer.mix_until(0.01).unwrap();
        assert!(block.samples.iter().all(|s| s.abs() < 1.0));
    }

    #[test]
    fn reports_and_resets_peak_levels() {
        let mut mixer = mixer(true, true);
        mixer.push(SourceKind::Microphone, 0.0, &[0.1, -0.6, 0.2, 0.0]);
        let levels = mixer.take_levels();
        assert_eq!(levels.microphone, 0.6);
        assert_eq!(levels.system_audio, 0.0);
        assert_eq!(mixer.take_levels(), Levels::default());
    }
}
