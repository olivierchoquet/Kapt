//! Pause/resume timestamp logic, kept free of any pipeline access so it can be
//! unit tested.

use gstreamer::ClockTime;

/// Pause bookkeeping shared between the main task and the pad probe.
///
/// While paused, frames are dropped. On resume, the pause duration is added to
/// `offset`, which is subtracted from every following timestamp: the recording
/// continues seamlessly instead of showing a frozen frame for the whole pause.
///
/// All times are pipeline running times, the same clock as the frame
/// timestamps (pipewiresrc runs with `do-timestamp=true`).
#[derive(Default)]
pub struct PauseState {
    paused: bool,
    /// Running time at which the current pause started.
    paused_at: ClockTime,
    /// Running time of the last resume: older frames still in flight are dropped.
    resumed_at: ClockTime,
    /// Total duration of all pauses so far.
    offset: ClockTime,
}

/// Result of [`PauseState::toggle`].
#[derive(Debug, PartialEq)]
pub enum Toggle {
    Paused,
    /// Recording again, after a pause of this duration.
    Resumed(ClockTime),
}

impl PauseState {
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Total duration of all pauses so far, to subtract from timestamps.
    pub fn offset(&self) -> ClockTime {
        self.offset
    }

    /// Switches between recording and paused at running time `now`.
    pub fn toggle(&mut self, now: ClockTime) -> Toggle {
        if self.paused {
            let pause = now.saturating_sub(self.paused_at);
            self.offset += pause;
            self.resumed_at = now;
            self.paused = false;
            Toggle::Resumed(pause)
        } else {
            self.paused_at = now;
            self.paused = true;
            Toggle::Paused
        }
    }

    /// New timestamp of a frame captured at `pts`, or `None` to drop it:
    /// it was captured while paused, or before the last resume and is only
    /// arriving now.
    pub fn shift(&self, pts: ClockTime) -> Option<ClockTime> {
        if self.paused || pts < self.resumed_at {
            return None;
        }
        Some(pts.saturating_sub(self.offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> ClockTime {
        ClockTime::from_mseconds(value)
    }

    #[test]
    fn frames_unchanged_without_pause() {
        let state = PauseState::default();
        assert_eq!(state.shift(ms(0)), Some(ms(0)));
        assert_eq!(state.shift(ms(1500)), Some(ms(1500)));
    }

    #[test]
    fn frames_dropped_while_paused() {
        let mut state = PauseState::default();
        assert_eq!(state.toggle(ms(1000)), Toggle::Paused);
        assert!(state.is_paused());
        assert_eq!(state.shift(ms(1200)), None);
    }

    #[test]
    fn frames_shifted_by_pause_duration_after_resume() {
        let mut state = PauseState::default();
        state.toggle(ms(1000));
        assert_eq!(state.toggle(ms(3000)), Toggle::Resumed(ms(2000)));
        assert!(!state.is_paused());
        // Captured right at the resume: continues where the pause started
        assert_eq!(state.shift(ms(3000)), Some(ms(1000)));
        assert_eq!(state.shift(ms(3040)), Some(ms(1040)));
    }

    #[test]
    fn frames_in_flight_from_before_resume_are_dropped() {
        let mut state = PauseState::default();
        state.toggle(ms(1000));
        state.toggle(ms(3000));
        // Captured during the pause, arriving after the resume
        assert_eq!(state.shift(ms(2990)), None);
    }

    #[test]
    fn pauses_add_up() {
        let mut state = PauseState::default();
        state.toggle(ms(1000));
        state.toggle(ms(2000)); // 1 s pause
        state.toggle(ms(5000));
        assert_eq!(state.toggle(ms(8000)), Toggle::Resumed(ms(3000)));
        assert_eq!(state.offset(), ms(4000));
        assert_eq!(state.shift(ms(8000)), Some(ms(4000)));
    }

    #[test]
    fn pause_at_start() {
        let mut state = PauseState::default();
        state.toggle(ms(0));
        state.toggle(ms(2500));
        assert_eq!(state.shift(ms(2500)), Some(ms(0)));
    }

    #[test]
    fn instant_pause_changes_nothing() {
        let mut state = PauseState::default();
        state.toggle(ms(1000));
        assert_eq!(state.toggle(ms(1000)), Toggle::Resumed(ms(0)));
        assert_eq!(state.shift(ms(1000)), Some(ms(1000)));
    }

    #[test]
    fn timestamps_stay_increasing_across_pauses() {
        let mut state = PauseState::default();
        let mut last = None;
        // 25 fps for 10 s (a frame every 40 ms), paused from 2 s to 4 s and from 6 s to 6.52 s
        for frame in 0..250 {
            let now = ms(frame * 40);
            if [2000, 4000, 6000, 6520].contains(&(frame * 40)) {
                state.toggle(now);
            }
            if let Some(pts) = state.shift(now) {
                if let Some(last) = last {
                    assert!(pts > last, "{pts} after {last}");
                }
                last = Some(pts);
            }
        }
        // Last frame at 9.96 s, minus 2.52 s of pauses
        assert_eq!(last, Some(ms(9960 - 2520)));
    }
}
