//! Where item keys get their randomness, injected like the clock so tests can choose the keys.
//!
//! Secrets (session tokens, password salts) don't come from here: they always use a generator the system seeds.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::ServiceError;

/// Random bytes for item keys.
pub trait RandomSource: Send + Sync {
    /// Fills `bytes` with random bytes, or fails with [`ServiceError::RandomSource`].
    fn fill(&self, bytes: &mut [u8]) -> Result<(), ServiceError>;
}

/// The operating system's random source.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemRandom;

impl RandomSource for SystemRandom {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), ServiceError> {
        getrandom::fill(bytes).map_err(|e| ServiceError::RandomSource(e.to_string()))
    }
}

/// Hands out the bytes it was given, in order, and fails once they run out. For tests.
#[derive(Debug, Default)]
pub struct ScriptedRandom(Mutex<VecDeque<u8>>);

impl ScriptedRandom {
    pub fn new(bytes: impl IntoIterator<Item = u8>) -> Self {
        Self(Mutex::new(bytes.into_iter().collect()))
    }

    /// Adds bytes after the ones not yet handed out.
    pub fn push(&self, bytes: impl IntoIterator<Item = u8>) {
        self.queue().extend(bytes);
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, VecDeque<u8>> {
        // A panic while holding the lock leaves the queue whole, so a poisoned lock is still usable.
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl RandomSource for ScriptedRandom {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), ServiceError> {
        let mut queue = self.queue();
        if queue.len() < bytes.len() {
            return Err(ServiceError::RandomSource("scripted bytes ran out".to_owned()));
        }
        let len = bytes.len();
        for (byte, next) in bytes.iter_mut().zip(queue.drain(..len)) {
            *byte = next;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn scripted_bytes_come_out_in_order_and_then_run_out() {
        let random = ScriptedRandom::new([1, 2, 3]);
        random.push([4]);
        let mut first = [0u8; 2];
        let mut second = [0u8; 2];
        let mut third = [0u8; 1];

        expect_that!(random.fill(&mut first), ok(anything()));
        expect_that!(random.fill(&mut second), ok(anything()));
        expect_that!(random.fill(&mut third), err(anything()));

        expect_that!(first, eq([1, 2]));
        expect_that!(second, eq([3, 4]));
    }
}
