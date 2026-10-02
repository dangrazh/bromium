//! GUI-only scheduling: provider work and live tree freshness remain in uitree.
use std::time::{Duration, Instant};

pub const POLL_INTERVAL: Duration = Duration::from_millis(100);
const SETTLE_TIME: Duration = Duration::from_millis(100);
const MOVING_INTERVAL: Duration = Duration::from_millis(250);
const STATIONARY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub window: isize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Live,
    Tracking,
    Paused,
}

#[derive(Clone, Copy, Debug)]
struct Request {
    generation: u64,
    pointer: Pointer,
}

#[derive(Default)]
pub struct CursorTracking {
    mode: Mode,
    generation: u64,
    latest: Option<Pointer>,
    changed_at: Option<Instant>,
    not_before: Option<Instant>,
    pending: Option<Request>,
}

impl CursorTracking {
    pub fn is_tracking(&self) -> bool {
        self.mode == Mode::Tracking
    }
    pub fn is_paused(&self) -> bool {
        self.mode == Mode::Paused
    }
    // Tracking renders successful point snapshots, not unrelated background commits.
    pub fn follows_background(&self) -> bool {
        self.mode == Mode::Live
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn set_tracking(&mut self, enabled: bool) {
        if enabled == self.is_tracking() {
            return;
        }
        self.change_mode(if enabled {
            Mode::Tracking
        } else {
            Mode::Paused
        });
    }

    pub fn refresh(&mut self) {
        self.change_mode(Mode::Live);
    }

    fn change_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.generation = self.generation.wrapping_add(1);
        self.latest = None;
        self.changed_at = None;
        self.not_before = None;
        // Retain the in-flight slot until its worker returns. Toggling tracking
        // cannot create concurrent workers or resurrect a previous session's hit.
    }

    pub fn observe(&mut self, pointer: Option<Pointer>, now: Instant) {
        let pointer = pointer.filter(|_| self.is_tracking());
        if self.latest != pointer {
            self.latest = pointer;
            self.changed_at = Some(now);
            // Motion should not inherit the longer stationary retry delay.
            self.not_before = self.not_before.map(|at| at.min(now + MOVING_INTERVAL));
        }
    }

    pub fn request(&mut self, now: Instant) -> Option<Pointer> {
        if !self.is_tracking()
            || self.pending.is_some()
            || self.not_before.is_some_and(|at| now < at)
            || self
                .changed_at
                .is_none_or(|at| now.saturating_duration_since(at) < SETTLE_TIME)
        {
            return None;
        }
        let pointer = self.latest?;
        self.pending = Some(Request {
            generation: self.generation,
            pointer,
        });
        Some(pointer)
    }

    /// Accept only the current session and current native window/coordinates.
    /// Errors have the same cooldown as successes, avoiding a stale-query loop.
    pub fn complete(&mut self, now: Instant) -> bool {
        let Some(request) = self.pending.take() else {
            return false;
        };
        let matches = self.is_tracking()
            && request.generation == self.generation
            && self.latest == Some(request.pointer);
        self.not_before = Some(
            now + if matches {
                STATIONARY_INTERVAL
            } else {
                MOVING_INTERVAL
            },
        );
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: Pointer = Pointer {
        x: 10,
        y: 20,
        window: 100,
    };
    const B: Pointer = Pointer {
        x: 30,
        y: 40,
        window: 100,
    };

    #[test]
    fn repaint_storm_is_single_flight_and_stationary_checks_are_bounded() {
        let start = Instant::now();
        let mut tracking = CursorTracking::default();
        tracking.set_tracking(true);
        let mut requests = 0;
        for millis in 0..10_000 {
            let now = start + Duration::from_millis(millis);
            tracking.observe(Some(A), now);
            if tracking.request(now).is_some() {
                requests += 1;
                assert!(tracking.request(now).is_none());
                assert!(tracking.complete(now));
            }
        }
        assert_eq!(
            requests, 10,
            "repaint frequency must not drive provider acquisition"
        );
    }

    #[test]
    fn slow_capture_coalesces_movement_and_rejects_outdated_position() {
        let now = Instant::now();
        let mut tracking = CursorTracking::default();
        tracking.set_tracking(true);
        tracking.observe(Some(A), now);
        assert_eq!(tracking.request(now + SETTLE_TIME), Some(A));
        for i in 1..100 {
            let later = now + Duration::from_secs(i);
            tracking.observe(Some(B), later);
            assert!(tracking.request(later).is_none());
        }
        let later = now + Duration::from_secs(100);
        assert!(!tracking.complete(later));
        assert_eq!(tracking.request(later + MOVING_INTERVAL), Some(B));
    }

    #[test]
    fn pause_freezes_background_and_discards_completion_even_after_restart() {
        let now = Instant::now();
        let mut tracking = CursorTracking::default();
        assert!(tracking.follows_background());
        tracking.set_tracking(true);
        tracking.observe(Some(A), now);
        tracking.request(now + SETTLE_TIME).unwrap();
        tracking.set_tracking(false);
        assert!(tracking.is_paused());
        assert!(!tracking.follows_background());
        assert!(tracking.request(now + Duration::from_secs(5)).is_none());
        tracking.set_tracking(true);
        tracking.observe(Some(A), now);
        assert!(tracking.request(now + SETTLE_TIME).is_none());
        assert!(!tracking.complete(now + Duration::from_secs(5)));
        tracking.set_tracking(false);
        tracking.refresh();
        assert!(tracking.follows_background());
        assert!(!tracking.is_tracking());
        assert!(!tracking.is_paused());
    }

    #[test]
    fn entering_inspector_or_changing_native_window_rejects_pending_hit() {
        for pointer in [None, Some(Pointer { window: 200, ..A })] {
            let now = Instant::now();
            let mut tracking = CursorTracking::default();
            tracking.set_tracking(true);
            tracking.observe(Some(A), now);
            tracking.request(now + SETTLE_TIME).unwrap();
            tracking.observe(pointer, now + SETTLE_TIME);
            assert!(!tracking.complete(now + SETTLE_TIME));
        }
    }

    #[test]
    fn refresh_invalidates_pending_tracking_result() {
        let now = Instant::now();
        let mut tracking = CursorTracking::default();
        tracking.set_tracking(true);
        tracking.observe(Some(A), now);
        tracking.request(now + SETTLE_TIME).unwrap();
        tracking.refresh();
        assert!(!tracking.complete(now + SETTLE_TIME));
        assert!(tracking.follows_background());
    }
}
