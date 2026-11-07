//! How often an account may be tried. Sign-in attempts are counted per email address in fixed windows: the first
//! attempt opens a window, and once an address has used up its attempts, further ones are refused until the
//! window ends, before any password is checked. A successful sign-in ends the window.
//!
//! Attempts are counted whether or not the address has an account, so being refused says nothing about whether
//! one exists. Refused attempts are counted too but don't extend the window.
//!
//! The limit is per address, so someone who knows an address can keep its owner out by using up its attempts at
//! the start of every window. That is the price of limiting guesses at one account no matter where they come from;
//! a limit per client would need the client's address, which the server behind a proxy doesn't know.

use crate::{Duration, Timestamp};

/// The attempts allowed per window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignInLimit {
    pub max_attempts: u32,
    pub window: Duration,
}

/// The attempts counted for one address, the current one included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignInAttempts {
    pub count: u32,
    pub window_ends_at: Timestamp,
}

impl SignInLimit {
    /// Five attempts per quarter of an hour: plenty for someone recalling their password, and a few hundred guesses
    /// a day for someone trying to guess it.
    pub const DEFAULT: Self = Self {
        max_attempts: 5,
        window: Duration::from_minutes(15),
    };

    /// When a window opened by an attempt at `now` ends. `None` past the supported range.
    pub fn window_end(self, now: Timestamp) -> Option<Timestamp> {
        now.checked_add(self.window)
    }

    /// `Ok` when the attempt may go ahead, or how long until the next one may.
    pub fn check(self, attempts: SignInAttempts, now: Timestamp) -> Result<(), Duration> {
        let wait = attempts.window_ends_at.since(now);
        if attempts.count > self.max_attempts && wait > Duration::ZERO {
            Err(wait)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn at(minutes: i64) -> Timestamp {
        Timestamp::EPOCH
            .checked_add(Duration::from_minutes(minutes))
            .unwrap_or(Timestamp::EPOCH)
    }

    #[gtest]
    fn refuses_attempts_past_the_limit_until_the_window_ends() {
        let limit = SignInLimit::DEFAULT;
        let attempts = |count| SignInAttempts {
            count,
            window_ends_at: at(15),
        };

        expect_that!(limit.check(attempts(5), at(1)), ok(anything()));
        expect_that!(limit.check(attempts(6), at(1)), err(eq(Duration::from_minutes(14))));
        expect_that!(limit.check(attempts(40), at(15)), ok(anything()));
    }
}
