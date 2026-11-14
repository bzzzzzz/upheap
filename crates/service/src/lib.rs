//! Use cases for Upheap.
//!
//! Every use case takes the caller's [`Principal`], checks the permission it needs before anything else, runs
//! in one transaction, and records its audit entry in that same transaction.

mod access;
pub mod account_history;
pub mod across_heaps;
mod audit;
pub mod audit_log;
pub mod auth;
mod clock;
pub mod comments;
mod error;
mod figures;
pub mod handles;
pub mod heaps;
pub mod items;
pub mod me;
mod moves;
pub mod oauth;
pub mod oidc;
pub mod parameters;
pub mod password_reset;
mod personal;
pub mod questions;
mod random;
pub mod saved_views;
mod splitting;
pub mod steps;
pub mod tags;
pub mod teams;
pub mod turns;
pub mod users;
pub mod waiting;
pub mod work;

use std::sync::{Arc, Mutex};

use domain::sign_in::SignInLimit;
use domain::{Duration, RunId, Timestamp, UserId};
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
    /// The run the request is made in, when an agent makes it (the `Upheap-Run` header). Only the API sets it; a
    /// write made in a run is held to what a run may do (architecture.md, Runs).
    pub run: Option<RunId>,
}

/// Deployment settings the use cases depend on. All of them are about accounts.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Whether anyone can create a local account.
    pub allow_registration: bool,
    /// How long a web session lasts after sign-in.
    pub session_ttl: Duration,
    /// How many sign-in attempts an email address gets.
    pub sign_in_limit: SignInLimit,
    /// The OpenID Connect provider people may sign in through, if the operator set one.
    pub oidc: Option<oidc::OidcSettings>,
}

impl Default for Settings {
    /// Registration closed, 30-day sessions, the default sign-in limit and no provider.
    fn default() -> Self {
        Self {
            allow_registration: false,
            session_ttl: Duration::from_days(30),
            sign_in_limit: SignInLimit::DEFAULT,
            oidc: None,
        }
    }
}

/// The mail the server sends: the mailer, and the address links in mail start with.
#[derive(Debug)]
struct MailSettings {
    mailer: Arc<mail::Mailer>,
    /// `BASE_URL`, without a trailing slash.
    base_url: String,
}

/// Entry point to every use case. Cheap to clone.
#[derive(Clone)]
pub struct Services {
    db: DatabaseConnection,
    clock: Arc<dyn Clock>,
    /// Randomness for item keys.
    random: Arc<dyn RandomSource>,
    settings: Arc<Settings>,
    /// Where mail goes, when the server has mail set up.
    mail: Option<Arc<MailSettings>>,
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
            mail: None,
            id_context: Arc::new(Mutex::new(uuid::ContextV7::new())),
        }
    }

    /// Draws item keys from `random` in place of the system's random source.
    #[must_use]
    pub fn with_random(mut self, random: Arc<dyn RandomSource>) -> Self {
        self.random = random;
        self
    }

    /// Sends mail through `mailer`, with links in it starting with `base_url`. Without it mail is off.
    #[must_use]
    pub fn with_mail(mut self, mailer: Arc<mail::Mailer>, base_url: String) -> Self {
        self.mail = Some(Arc::new(MailSettings { mailer, base_url }));
        self
    }

    /// Whether this server mails password reset links.
    pub fn mails_reset_links(&self) -> bool {
        self.mail.is_some()
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
