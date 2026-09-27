//! The lifecycle of a recording as an explicit state machine.
//!
//! ```text
//!            start            started
//!   Idle ───────────▶ Starting ───────▶ Recording ◀──resume── Paused
//!    ▲                   │                 │  └────pause────────▲
//!    │                   │ fail            │ stop / fail          │ stop / fail
//!    │                   ▼                 ▼                      │
//!    └──── finished ─── Stopping ◀────────────────────────────────┘
//! ```
//!
//! Any failure while starting returns straight to `Idle`; failures while
//! recording go through `Stopping` so the file can still be finalized.

use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingState {
    Idle,
    Starting,
    Recording,
    Paused,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingEvent {
    Start,
    Started,
    Pause,
    Resume,
    Stop,
    Fail,
    Finished,
}

impl RecordingState {
    /// Returns the state that follows `event`, or an error if the event is
    /// not valid in the current state.
    pub fn next(self, event: RecordingEvent) -> AppResult<RecordingState> {
        use RecordingEvent as E;
        use RecordingState as S;

        let next = match (self, event) {
            (S::Idle, E::Start) => S::Starting,
            (S::Starting, E::Started) => S::Recording,
            (S::Starting, E::Fail) => S::Idle,
            (S::Recording, E::Pause) => S::Paused,
            (S::Paused, E::Resume) => S::Recording,
            (S::Recording | S::Paused, E::Stop | E::Fail) => S::Stopping,
            (S::Stopping, E::Finished | E::Fail) => S::Idle,
            (state, event) => {
                return Err(AppError::InvalidState(format!(
                    "cannot {} while {}",
                    event.verb(),
                    state.describe()
                )))
            }
        };
        Ok(next)
    }

    pub fn is_active(self) -> bool {
        !matches!(self, RecordingState::Idle)
    }

    fn describe(self) -> &'static str {
        match self {
            RecordingState::Idle => "not recording",
            RecordingState::Starting => "starting",
            RecordingState::Recording => "recording",
            RecordingState::Paused => "paused",
            RecordingState::Stopping => "stopping",
        }
    }
}

impl RecordingEvent {
    fn verb(self) -> &'static str {
        match self {
            RecordingEvent::Start => "start recording",
            RecordingEvent::Started => "begin capturing",
            RecordingEvent::Pause => "pause",
            RecordingEvent::Resume => "resume",
            RecordingEvent::Stop => "stop",
            RecordingEvent::Fail => "fail",
            RecordingEvent::Finished => "finish",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RecordingEvent as E;
    use super::RecordingState as S;
    use super::*;

    fn run(events: &[RecordingEvent]) -> AppResult<RecordingState> {
        events
            .iter()
            .try_fold(S::Idle, |state, &event| state.next(event))
    }

    #[test]
    fn happy_path_returns_to_idle() {
        assert_eq!(run(&[E::Start, E::Started]).unwrap(), S::Recording);
        assert_eq!(run(&[E::Start, E::Started, E::Stop]).unwrap(), S::Stopping);
        assert_eq!(
            run(&[E::Start, E::Started, E::Stop, E::Finished]).unwrap(),
            S::Idle
        );
    }

    #[test]
    fn pause_and_resume_can_repeat() {
        let state = run(&[
            E::Start,
            E::Started,
            E::Pause,
            E::Resume,
            E::Pause,
            E::Resume,
        ])
        .unwrap();
        assert_eq!(state, S::Recording);
    }

    #[test]
    fn can_stop_while_paused() {
        assert_eq!(
            run(&[E::Start, E::Started, E::Pause, E::Stop]).unwrap(),
            S::Stopping
        );
    }

    #[test]
    fn failure_while_starting_returns_to_idle() {
        assert_eq!(run(&[E::Start, E::Fail]).unwrap(), S::Idle);
    }

    #[test]
    fn failure_while_recording_still_finalizes() {
        assert_eq!(run(&[E::Start, E::Started, E::Fail]).unwrap(), S::Stopping);
        assert_eq!(
            run(&[E::Start, E::Started, E::Pause, E::Fail]).unwrap(),
            S::Stopping
        );
    }

    #[test]
    fn rejects_invalid_transitions() {
        assert!(S::Idle.next(E::Stop).is_err());
        assert!(S::Idle.next(E::Pause).is_err());
        assert!(S::Recording.next(E::Start).is_err());
        assert!(S::Recording.next(E::Resume).is_err());
        assert!(S::Paused.next(E::Pause).is_err());
        assert!(S::Stopping.next(E::Start).is_err());
        assert!(S::Starting.next(E::Pause).is_err());
    }

    #[test]
    fn invalid_transition_errors_are_readable() {
        let error = S::Idle.next(E::Pause).unwrap_err();
        assert_eq!(error.to_string(), "cannot pause while not recording");
    }

    #[test]
    fn only_idle_is_inactive() {
        assert!(!S::Idle.is_active());
        for state in [S::Starting, S::Recording, S::Paused, S::Stopping] {
            assert!(state.is_active());
        }
    }
}
