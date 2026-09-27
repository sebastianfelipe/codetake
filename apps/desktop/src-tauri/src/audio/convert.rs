//! Conversion of captured audio into the mixer format.

use super::{CHANNELS, SAMPLE_RATE};

/// Layout of the samples delivered by a capture API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    F32,
    I16,
    I32,
}

/// Describes a captured buffer so it can be converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputFormat {
    pub sample_rate: u32,
    pub channels: usize,
    pub format: SampleFormat,
}

/// Decodes raw little-endian bytes into floats in -1.0..=1.0.
pub fn decode_samples(bytes: &[u8], format: SampleFormat) -> Vec<f32> {
    match format {
        SampleFormat::F32 => bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        SampleFormat::I16 => bytes
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect(),
        SampleFormat::I32 => bytes
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0)
            .collect(),
    }
}

/// Interleaves separate channel planes (`planes[channel][frame]`).
pub fn interleave(planes: &[Vec<f32>]) -> Vec<f32> {
    let frames = planes.iter().map(Vec::len).min().unwrap_or(0);
    let mut out = Vec::with_capacity(frames * planes.len());
    for frame in 0..frames {
        for plane in planes {
            out.push(plane[frame]);
        }
    }
    out
}

/// Converts interleaved audio with any channel count to interleaved stereo.
///
/// Mono is duplicated to both channels; for more than two channels the first
/// two are kept (front left/right in every common layout).
pub fn to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        0 => Vec::new(),
        1 => samples.iter().flat_map(|&s| [s, s]).collect(),
        2 => samples.to_vec(),
        n => samples
            .chunks_exact(n)
            .flat_map(|frame| [frame[0], frame[1]])
            .collect(),
    }
}

/// Streaming linear resampler for interleaved stereo audio.
///
/// Keeps state between calls so consecutive buffers join without clicks.
/// Linear interpolation is adequate for voice and system audio and avoids
/// pulling in a DSP dependency; capture APIs are asked for 48 kHz first so
/// this is only a fallback.
#[derive(Debug, Clone)]
pub struct Resampler {
    step: f64,
    position: f64,
    previous: [f32; CHANNELS],
}

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        Self {
            step: f64::from(input_rate) / f64::from(SAMPLE_RATE),
            position: 0.0,
            previous: [0.0; CHANNELS],
        }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.step - 1.0).abs() < f64::EPSILON
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.is_passthrough() {
            return input.to_vec();
        }
        let frames = input.len() / CHANNELS;
        if frames == 0 {
            return Vec::new();
        }
        let frame = |index: isize| -> [f32; CHANNELS] {
            if index < 0 {
                self.previous
            } else {
                let i = index as usize * CHANNELS;
                [input[i], input[i + 1]]
            }
        };

        // `position` is measured relative to the first frame of `input`, and
        // may start at -1.0..0.0, interpolating from the previous buffer.
        let mut out = Vec::with_capacity((frames as f64 / self.step) as usize * CHANNELS + 4);
        while self.position < (frames - 1) as f64 {
            let base = self.position.floor();
            let t = (self.position - base) as f32;
            let a = frame(base as isize);
            let b = frame(base as isize + 1);
            out.push(a[0] + (b[0] - a[0]) * t);
            out.push(a[1] + (b[1] - a[1]) * t);
            self.position += self.step;
        }
        self.position -= frames as f64;
        self.previous = frame(frames as isize - 1);
        out
    }
}

/// Converts one captured buffer (already decoded and interleaved) into
/// 48 kHz interleaved stereo.
#[derive(Debug, Clone)]
pub struct Converter {
    input: InputFormat,
    resampler: Resampler,
}

impl Converter {
    pub fn new(input: InputFormat) -> Self {
        Self {
            input,
            resampler: Resampler::new(input.sample_rate),
        }
    }

    pub fn input_format(&self) -> InputFormat {
        self.input
    }

    pub fn convert(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let stereo = to_stereo(interleaved, self.input.channels);
        self.resampler.process(&stereo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_integer_and_float_samples() {
        let bytes: Vec<u8> = [i16::MAX, i16::MIN, 0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let decoded = decode_samples(&bytes, SampleFormat::I16);
        assert!((decoded[0] - 1.0).abs() < 0.001);
        assert_eq!(decoded[1], -1.0);
        assert_eq!(decoded[2], 0.0);

        let bytes: Vec<u8> = [0.5_f32, -0.25]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert_eq!(decode_samples(&bytes, SampleFormat::F32), vec![0.5, -0.25]);
    }

    #[test]
    fn interleaves_planes() {
        let planes = vec![vec![1.0, 2.0, 3.0], vec![-1.0, -2.0, -3.0]];
        assert_eq!(interleave(&planes), vec![1.0, -1.0, 2.0, -2.0, 3.0, -3.0]);
    }

    #[test]
    fn converts_channel_layouts_to_stereo() {
        assert_eq!(to_stereo(&[0.1, 0.2], 1), vec![0.1, 0.1, 0.2, 0.2]);
        assert_eq!(to_stereo(&[0.1, 0.2], 2), vec![0.1, 0.2]);
        assert_eq!(
            to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3),
            vec![1.0, 2.0, 4.0, 5.0]
        );
    }

    #[test]
    fn resampler_passes_48k_through_unchanged() {
        let mut resampler = Resampler::new(48_000);
        let input = vec![0.1, 0.2, 0.3, 0.4];
        assert_eq!(resampler.process(&input), input);
    }

    #[test]
    fn resampler_produces_the_expected_number_of_frames() {
        let mut resampler = Resampler::new(44_100);
        let input = vec![0.0; 44_100 * CHANNELS];
        let mut total = 0;
        for chunk in input.chunks(441 * CHANNELS) {
            total += resampler.process(chunk).len() / CHANNELS;
        }
        assert!((total as i64 - 48_000).abs() <= 2, "got {total} frames");
    }

    #[test]
    fn resampler_is_continuous_across_buffers() {
        // A slow ramp resampled in small chunks must stay monotonic.
        let mut resampler = Resampler::new(16_000);
        let ramp: Vec<f32> = (0..1600).flat_map(|i| [i as f32, i as f32]).collect();
        let mut out = Vec::new();
        for chunk in ramp.chunks(10 * CHANNELS) {
            out.extend(resampler.process(chunk));
        }
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(left.windows(2).all(|w| w[1] >= w[0]));
        assert!(left.len() > 4700);
    }

    #[test]
    fn converter_handles_mono_44k() {
        let mut converter = Converter::new(InputFormat {
            sample_rate: 44_100,
            channels: 1,
            format: SampleFormat::F32,
        });
        let out = converter.convert(&vec![0.5; 4410]);
        assert_eq!(out.len() % CHANNELS, 0);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}
