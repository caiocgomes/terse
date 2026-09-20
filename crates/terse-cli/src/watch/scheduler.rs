//! Pure trailing-debounce build scheduler: no filesystem, no threads, no
//! real sleeping. Driven entirely by injected [`Instant`]s so its exact
//! debounce/coalescing behavior is unit-testable without real time passing.

use std::time::{Duration, Instant};

/// Trailing debounce window: a burst of events restarts the window on every
/// event, so a rebuild starts 150 ms after the *last* event in a burst, not
/// the first.
pub const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerState {
    /// No pending event and no build in flight.
    Idle,
    /// At least one event has arrived; waiting out the debounce window.
    Debouncing,
    /// A build is running; no further event has arrived yet.
    Building,
    /// A build is running and at least one more event has arrived since it
    /// started; exactly one rebuild will be scheduled once it finishes.
    BuildingWithPending,
}

/// Schedules at most one active build and at most one pending successor,
/// coalescing any number of events arriving during either the debounce
/// window or an in-flight build into that single successor.
pub struct Scheduler {
    state: SchedulerState,
    last_event: Option<Instant>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self { state: SchedulerState::Idle, last_event: None }
    }

    pub fn state(&self) -> SchedulerState {
        self.state
    }

    /// Marks the very first build (watch's normal initial-build behavior),
    /// which runs immediately rather than waiting for an event or a
    /// debounce window.
    pub fn start_initial_build(&mut self) {
        self.state = SchedulerState::Building;
    }

    /// Records a relevant filesystem event at `now`.
    pub fn on_event(&mut self, now: Instant) {
        self.last_event = Some(now);
        self.state = match self.state {
            SchedulerState::Idle | SchedulerState::Debouncing => SchedulerState::Debouncing,
            SchedulerState::Building | SchedulerState::BuildingWithPending => {
                SchedulerState::BuildingWithPending
            }
        };
    }

    /// Checked periodically with the current time. Returns `true` exactly
    /// once the debounce window has elapsed since the last event, at which
    /// point the caller should start a build and the scheduler transitions
    /// to `Building`.
    pub fn poll(&mut self, now: Instant) -> bool {
        match self.state {
            SchedulerState::Debouncing => {
                let last = self.last_event.expect("debouncing implies a recorded event");
                if now.saturating_duration_since(last) >= DEBOUNCE {
                    self.state = SchedulerState::Building;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Reports that the in-flight build has finished. If at least one event
    /// arrived during it, starts a fresh debounce window from `now` for the
    /// single coalesced successor; otherwise returns to idle.
    pub fn build_finished(&mut self, now: Instant) {
        match self.state {
            SchedulerState::Building => self.state = SchedulerState::Idle,
            SchedulerState::BuildingWithPending => {
                self.state = SchedulerState::Debouncing;
                self.last_event = Some(now);
            }
            SchedulerState::Idle | SchedulerState::Debouncing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_events_use_trailing_debounce() {
        let base = Instant::now();
        let mut s = Scheduler::new();

        s.on_event(base);
        assert!(!s.poll(base + Duration::from_millis(50)));

        // A second event within the window restarts it from its own time.
        s.on_event(base + Duration::from_millis(100));
        assert!(!s.poll(base + Duration::from_millis(200))); // only 100ms since the last event
        assert!(s.poll(base + Duration::from_millis(260))); // 160ms since the last event
        assert_eq!(s.state(), SchedulerState::Building);
    }

    #[test]
    fn test_burst_of_events_coalesces_into_one_rebuild() {
        let base = Instant::now();
        let mut s = Scheduler::new();
        for i in 0..10u32 {
            s.on_event(base + Duration::from_millis(i as u64 * 5));
        }
        // Not enough time has passed since the very last event yet.
        assert!(!s.poll(base + Duration::from_millis(100)));
        assert!(s.poll(base + Duration::from_millis(46 + 150)));
        assert_eq!(s.state(), SchedulerState::Building);
    }

    #[test]
    fn test_one_active_build_and_at_most_one_pending_successor() {
        let base = Instant::now();
        let mut s = Scheduler::new();
        s.start_initial_build();
        assert_eq!(s.state(), SchedulerState::Building);

        // Three events arrive while the initial build is still running.
        s.on_event(base);
        s.on_event(base + Duration::from_millis(10));
        s.on_event(base + Duration::from_millis(20));
        assert_eq!(s.state(), SchedulerState::BuildingWithPending);

        s.build_finished(base + Duration::from_millis(30));
        assert_eq!(s.state(), SchedulerState::Debouncing);

        // Exactly one rebuild is scheduled, debounced from the finish time,
        // not from any of the three original events.
        assert!(!s.poll(base + Duration::from_millis(100)));
        assert!(s.poll(base + Duration::from_millis(30 + 150)));
        assert_eq!(s.state(), SchedulerState::Building);
    }

    #[test]
    fn test_idle_after_a_clean_build_with_no_new_events() {
        let base = Instant::now();
        let mut s = Scheduler::new();
        s.start_initial_build();
        s.build_finished(base);
        assert_eq!(s.state(), SchedulerState::Idle);
        assert!(!s.poll(base + Duration::from_secs(1)));
    }
}
