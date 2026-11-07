//! Use cases for Upheap.
//!
//! Every use case takes the caller's [`Principal`], checks the permission it needs before anything else, runs
//! in one transaction, and records its audit entry in that same transaction.
//!
//! The `accounts` feature (on by default) adds what only a shared server needs: registration, sign-in and
//! sessions, team management and user lookup. The local build leaves it out and has one user,
//! [`Services::open_local_user`].

mod access;
mod audit;
pub mod audit_log;
#[cfg(feature = "accounts")]
pub mod auth;
mod clock;
mod error;
pub mod handles;
pub mod heaps;
pub mod items;
pub mod me;
pub mod parameters;
mod personal;
mod random;
pub mod saved_views;
pub mod tags;
pub mod teams;
#[cfg(feature = "accounts")]
pub mod users;

use std::sync::{Arc, Mutex};

use domain::sign_in::SignInLimit;
use domain::{Duration, Timestamp, UserId};
use store::DatabaseConnection;
use uuid::Uuid;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::ServiceError;
pub use random::{RandomSource, ScriptedRandom, SystemRandom};
pub use store::StoreError;

/// The authenticated caller of a use case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Principal {
    pub user_id: UserId,
}

/// Deployment settings the use cases depend on. All of them are about accounts, so the local build takes the
/// defaults.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Whether anyone can create a local account.
    pub allow_registration: bool,
    /// How long a web session lasts after sign-in.
    pub session_ttl: Duration,
    /// How many sign-in attempts an email address gets.
    pub sign_in_limit: SignInLimit,
}

impl Default for Settings {
    /// Registration closed, 30-day sessions and the default sign-in limit.
    fn default() -> Self {
        Self {
            allow_registration: false,
            session_ttl: Duration::from_days(30),
            sign_in_limit: SignInLimit::DEFAULT,
        }
    }
}

/// Entry point to every use case. Cheap to clone.
#[derive(Clone)]
pub struct Services {
    db: DatabaseConnection,
    clock: Arc<dyn Clock>,
    /// Randomness for item keys.
    random: Arc<dyn RandomSource>,
    // Only accounts read the settings.
    #[cfg_attr(not(feature = "accounts"), allow(dead_code))]
    settings: Arc<Settings>,
    /// Counts IDs made in the same millisecond, so they still sort in the order they were made.
    id_context: Arc<Mutex<uuid::ContextV7>>,
}

impl Services {
    pub fn new(db: DatabaseConnection, clock: Arc<dyn Clock>, settings: Settings) -> Self {
        Self {
            db,
            clock,
            random: Arc::new(SystemRandom),
            settings: Arc::new(settings),
            id_context: Arc::new(Mutex::new(uuid::ContextV7::new())),
        }
    }

    /// Draws item keys from `random` in place of the system's random source.
    #[must_use]
    pub fn with_random(mut self, random: Arc<dyn RandomSource>) -> Self {
        self.random = random;
        self
    }

    /// The current time from the injected clock.
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// Checks the database answers. Used by the readiness probe.
    pub async fn check_database(&self) -> Result<(), ServiceError> {
        store::ping(&self.db).await.map_err(ServiceError::from)
    }

    /// A new UUIDv7 whose time part comes from the injected clock, so IDs sort with `created_at`. IDs made in the
    /// same millisecond by this process sort in the order they were made, so audit entries of one use case, and of
    /// writes in quick succession, read back in order.
    fn new_id(&self, now: Timestamp) -> Uuid {
        let millis = u64::try_from(now.as_millis()).unwrap_or(0);
        let ts = uuid::Timestamp::from_unix(&*self.id_context, millis / 1000, ((millis % 1000) * 1_000_000) as u32);
        Uuid::new_v7(ts)
    }
}
