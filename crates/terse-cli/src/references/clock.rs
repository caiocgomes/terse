//! Injected monotonic clock, so the arXiv rate limiter (14.5) is testable
//! without real sleeps.

use std::time::{Duration, Instant};

pub trait Clock {
    fn now(&self) -> Instant;
    fn sleep(&mut self, duration: Duration);
}

pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

pub mod fake {
    use super::*;

    /// A virtual clock: `sleep` advances `now()` instantly instead of
    /// really waiting, while recording every requested duration so tests
    /// can assert the rate limiter waited the correct amount without the
    /// test itself taking any wall-clock time.
    pub struct FakeClock {
        pub now: Instant,
        pub slept: Vec<Duration>,
    }

    impl FakeClock {
        pub fn new() -> Self {
            FakeClock { now: Instant::now(), slept: Vec::new() }
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            self.now
        }

        fn sleep(&mut self, duration: Duration) {
            self.slept.push(duration);
            self.now += duration;
        }
    }
}
