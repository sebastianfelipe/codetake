//! Maps capture timestamps onto the output timeline, accounting for pauses.
//!
//! All capture sources on a platform timestamp their samples with the same
//! monotonic host clock (in seconds). The output file starts at media time 0
//! when recording starts, and paused intervals are removed from the timeline
//! so the recording continues seamlessly after a resume.

#[derive(Debug, Clone)]
pub struct MediaClock {
    start: f64,
    paused_total: f64,
    paused_since: Option<f64>,
    last_resume: f64,
}

impl MediaClock {
    pub fn new(start: f64) -> Self {
        Self {
            start,
            paused_total: 0.0,
            paused_since: None,
            last_resume: start,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_since.is_some()
    }

    pub fn pause(&mut self, now: f64) {
        if self.paused_since.is_none() {
            self.paused_since = Some(now);
        }
    }

    pub fn resume(&mut self, now: f64) {
        if let Some(since) = self.paused_since.take() {
            self.paused_total += (now - since).max(0.0);
            self.last_resume = now;
        }
    }

    /// Converts a host timestamp to media time, or `None` if the sample was
    /// captured before the recording started or while it was paused.
    pub fn media_time(&self, host_time: f64) -> Option<f64> {
        if self.paused_since.is_some() || host_time < self.last_resume {
            return None;
        }
        Some(host_time - self.start - self.paused_total)
    }

    /// Recorded duration (excluding pauses) as of `now`.
    pub fn elapsed(&self, now: f64) -> f64 {
        let paused_now = self
            .paused_since
            .map_or(0.0, |since| (now - since).max(0.0));
        (now - self.start - self.paused_total - paused_now).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_time_starts_at_zero() {
        let clock = MediaClock::new(100.0);
        assert_eq!(clock.media_time(100.0), Some(0.0));
        assert_eq!(clock.media_time(102.5), Some(2.5));
    }

    #[test]
    fn drops_samples_captured_before_start() {
        let clock = MediaClock::new(100.0);
        assert_eq!(clock.media_time(99.9), None);
    }

    #[test]
    fn pauses_are_removed_from_the_timeline() {
        let mut clock = MediaClock::new(100.0);
        clock.pause(105.0);
        assert!(clock.is_paused());
        assert_eq!(clock.media_time(106.0), None);
        clock.resume(110.0);
        assert!(!clock.is_paused());
        assert_eq!(clock.media_time(110.0), Some(5.0));
        assert_eq!(clock.media_time(111.0), Some(6.0));
    }

    #[test]
    fn samples_from_the_paused_interval_delivered_late_are_dropped() {
        let mut clock = MediaClock::new(0.0);
        clock.pause(5.0);
        clock.resume(10.0);
        // Captured during the pause but delivered after resume.
        assert_eq!(clock.media_time(7.0), None);
    }

    #[test]
    fn elapsed_excludes_pauses_including_the_current_one() {
        let mut clock = MediaClock::new(0.0);
        assert_eq!(clock.elapsed(4.0), 4.0);
        clock.pause(4.0);
        assert_eq!(clock.elapsed(9.0), 4.0);
        clock.resume(10.0);
        assert_eq!(clock.elapsed(12.0), 6.0);
    }

    #[test]
    fn repeated_pause_or_resume_calls_are_idempotent() {
        let mut clock = MediaClock::new(0.0);
        clock.pause(1.0);
        clock.pause(2.0);
        clock.resume(3.0);
        clock.resume(4.0);
        assert_eq!(clock.media_time(5.0), Some(3.0));
    }
}
