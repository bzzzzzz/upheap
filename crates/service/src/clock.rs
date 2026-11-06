use std::sync::atomic::{AtomicI64, Ordering};

use domain::{Duration, Timestamp};

/// Source of the current time. Injected so tests can control it.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// The system's wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        // The system clock is always well inside the supported range; the epoch is a safe floor if it isn't.
        Timestamp::from_millis(millis).unwrap_or(Timestamp::EPOCH)
    }
}

/// A clock that only moves when told to. For tests.
#[derive(Debug)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(start: Timestamp) -> Self {
        Self(AtomicI64::new(start.as_millis()))
    }

    pub fn advance(&self, by: Duration) {
        self.0.fetch_add(by.as_millis(), Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(self.0.load(Ordering::SeqCst)).unwrap_or(Timestamp::EPOCH)
    }
}
