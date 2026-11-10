//! Instants and durations in UTC milliseconds.

use std::fmt;

use jiff::civil::Date;
use jiff::tz::TimeZone;

/// A UTC instant in milliseconds since the Unix epoch.
///
/// Always within the range `jiff` can represent (years -9999 to 9999), which the constructor checks, so
/// conversions to calendar dates can't fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

/// A signed span of time in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(i64);

const MILLIS_PER_MINUTE: i64 = 60 * 1000;
const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;

impl Timestamp {
    /// 1970-01-01T00:00:00Z.
    pub const EPOCH: Self = Self(0);

    /// Returns `None` when `millis` is outside the supported range.
    pub fn from_millis(millis: i64) -> Option<Self> {
        jiff::Timestamp::from_millisecond(millis).ok().map(|_| Self(millis))
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// How long after `earlier` this instant is. Negative when `earlier` is actually later.
    pub const fn since(self, earlier: Self) -> Duration {
        // Both values are within jiff's range (about ±3.2e14 ms), so the difference can't overflow.
        Duration(self.0 - earlier.0)
    }

    /// Returns `None` when the result would leave the supported range.
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.0.checked_add(duration.0).and_then(Self::from_millis)
    }

    /// Returns `None` when the result would leave the supported range.
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        self.0.checked_sub(duration.0).and_then(Self::from_millis)
    }

    /// The last millisecond of `date` in `tz`, which is how dates without a time are stored.
    ///
    /// Returns `None` when the date is at the very end of the supported range.
    pub fn at_end_of_day(date: Date, tz: &TimeZone) -> Option<Self> {
        let next_midnight = date.tomorrow().ok()?.to_zoned(tz.clone()).ok()?;
        Self::from_millis(next_midnight.timestamp().as_millisecond() - 1)
    }

    pub fn to_jiff(self) -> jiff::Timestamp {
        #[allow(clippy::expect_used)]
        // Proven invariant: the constructor only accepts values jiff can represent.
        jiff::Timestamp::from_millisecond(self.0).expect("timestamp range is checked at construction")
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_jiff().fmt(f)
    }
}

impl Duration {
    pub const ZERO: Self = Self(0);

    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    pub const fn from_minutes(minutes: i64) -> Self {
        Self(minutes * MILLIS_PER_MINUTE)
    }

    pub const fn from_hours(hours: i64) -> Self {
        Self(hours * MILLIS_PER_HOUR)
    }

    pub const fn from_days(days: i64) -> Self {
        Self(days * MILLIS_PER_DAY)
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// Whole seconds, rounded up, so a wait given in seconds is never too short.
    pub const fn as_seconds_ceil(self) -> i64 {
        (self.0 + 999).div_euclid(1000)
    }

    /// Whole minutes, rounded up.
    pub const fn as_minutes_ceil(self) -> i64 {
        (self.0 + MILLIS_PER_MINUTE - 1).div_euclid(MILLIS_PER_MINUTE)
    }

    /// Length in days, with fractions.
    pub fn as_days_f64(self) -> f64 {
        self.0 as f64 / MILLIS_PER_DAY as f64
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn waits_round_up() {
        expect_that!(Duration::from_millis(60_001).as_minutes_ceil(), eq(2));
        expect_that!(Duration::from_millis(1_001).as_seconds_ceil(), eq(2));
        expect_that!(Duration::from_minutes(14).as_seconds_ceil(), eq(840));
    }

    #[gtest]
    fn end_of_day_in_berlin_is_stored_as_utc() {
        let tz = TimeZone::get("Europe/Berlin").expect("bundled tzdb has Berlin");
        let date = jiff::civil::date(2027, 3, 12);

        let stored = Timestamp::at_end_of_day(date, &tz).expect("in range");

        expect_that!(stored.to_jiff().to_string(), eq("2027-03-12T22:59:59.999Z"));
    }

    #[gtest]
    fn subtracting_a_duration_moves_back_and_stops_at_the_range_end() {
        let now = Timestamp::from_millis(2 * MILLIS_PER_DAY).expect("in range");

        expect_that!(
            now.checked_sub(Duration::from_days(1)),
            some(eq(Timestamp::from_millis(MILLIS_PER_DAY).expect("in range")))
        );
        expect_that!(
            Timestamp::EPOCH.checked_sub(Duration::from_days(i64::MAX / MILLIS_PER_DAY)),
            none()
        );
    }

    #[gtest]
    fn rejects_out_of_range_millis() {
        expect_that!(Timestamp::from_millis(i64::MAX), none());
        expect_that!(Timestamp::from_millis(0), some(anything()));
    }
}
