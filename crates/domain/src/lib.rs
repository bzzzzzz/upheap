//! Pure domain logic for Upheap.
//!
//! Nothing in this crate does IO, reads the clock or uses randomness. Time is always passed in, so every
//! result is reproducible.

pub mod audit;
pub mod due;
pub mod heap;
pub mod ids;
pub mod item;
pub mod permission;
pub mod ranking;
pub mod team;
pub mod text;
pub mod time;
pub mod user;
pub mod validation;

pub use ids::{HeapId, ItemId, TeamId, UserId};
pub use time::{Duration, Timestamp};
pub use validation::ValidationError;

/// Optimistic-concurrency version of a mutable row. Starts at 1 and goes up by one on every write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u32);

impl Version {
    pub const INITIAL: Self = Self(1);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The version a row has after one more write. Saturates instead of wrapping, which no real row reaches.
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}
