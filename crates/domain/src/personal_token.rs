//! Personal tokens: bearer tokens a person makes in the web app for a machine with no browser or keychain. Unlike
//! an OAuth sign-in they never rotate, so a copy on such a machine keeps working until it expires or is revoked.
//! A token's name is parsed as a device name is (`oauth::DeviceName`).

use crate::{Duration, Timestamp};

/// What a personal token's secret starts with. It tells one from an OAuth token and lets secret scanners find it.
pub const PERSONAL_TOKEN_PREFIX: &str = "uph_";

/// How stale `last_used_at` may get before a use writes it again, so a burst of requests writes once a minute.
pub const LAST_USED_PRECISION: Duration = Duration::from_minutes(1);

/// How long a token lasts from when it is made, as the person picks it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenExpiry {
    Days30,
    #[default]
    Days90,
    Year1,
    Never,
}

impl TokenExpiry {
    /// `None` for a token that never expires.
    pub const fn duration(self) -> Option<Duration> {
        match self {
            Self::Days30 => Some(Duration::from_days(30)),
            Self::Days90 => Some(Duration::from_days(90)),
            Self::Year1 => Some(Duration::from_days(365)),
            Self::Never => None,
        }
    }
}

/// Whether a token can be used, and if not, why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenState {
    Live,
    Revoked,
    /// Its time passed. A revoked token that also expired is `Revoked`.
    Expired,
}

impl TokenState {
    /// A token is live when it isn't revoked and has no end or one after `now`.
    pub fn of(revoked_at: Option<Timestamp>, expires_at: Option<Timestamp>, now: Timestamp) -> Self {
        if revoked_at.is_some() {
            Self::Revoked
        } else if expires_at.is_some_and(|end| end <= now) {
            Self::Expired
        } else {
            Self::Live
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn expiry_defaults_to_ninety_days() {
        expect_that!(TokenExpiry::default(), eq(TokenExpiry::Days90));
    }

    #[gtest]
    fn expiry_durations() {
        expect_that!(TokenExpiry::Days30.duration(), some(eq(Duration::from_days(30))));
        expect_that!(TokenExpiry::Days90.duration(), some(eq(Duration::from_days(90))));
        expect_that!(TokenExpiry::Year1.duration(), some(eq(Duration::from_days(365))));
        expect_that!(TokenExpiry::Never.duration(), none());
    }

    #[gtest]
    fn a_token_is_live_until_revoked_or_expired() {
        let now = Timestamp::from_millis(1_000).expect("in range");
        let before = Timestamp::from_millis(999).expect("in range");
        let after = Timestamp::from_millis(1_001).expect("in range");

        expect_that!(TokenState::of(None, None, now), eq(TokenState::Live));
        expect_that!(TokenState::of(None, Some(after), now), eq(TokenState::Live));
        expect_that!(TokenState::of(None, Some(now), now), eq(TokenState::Expired));
        expect_that!(TokenState::of(Some(before), None, now), eq(TokenState::Revoked));
        expect_that!(TokenState::of(Some(before), Some(before), now), eq(TokenState::Revoked));
    }
}
