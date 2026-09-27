//! Platform-independent audio processing.
//!
//! Every audio source is converted to the mixer format — interleaved stereo
//! 32-bit float at 48 kHz — before it reaches the [`mixer::AudioMixer`].

pub mod convert;
pub mod mixer;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Peak level of an interleaved buffer, in the 0.0..=1.0 range.
pub fn peak_level(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0_f32, |peak, s| peak.max(s.abs()))
        .min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_level_is_the_largest_magnitude_clamped_to_one() {
        assert_eq!(peak_level(&[]), 0.0);
        assert_eq!(peak_level(&[0.1, -0.5, 0.25]), 0.5);
        assert_eq!(peak_level(&[2.0]), 1.0);
    }
}
