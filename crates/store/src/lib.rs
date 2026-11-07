//! Persistence for Upheap.
//!
//! Repository functions take any connection or transaction (`&impl ConnectionTrait`), so the service layer
//! decides transaction boundaries. Rows are converted to domain types here, and rows that break a domain rule
//! are rejected as [`StoreError::CorruptRow`] rather than passed on.

pub mod audit;
mod codes;
mod entities;
mod error;
pub mod heaps;
pub mod items;
mod migration;
pub mod parameters;
#[cfg(feature = "proxy")]
pub mod proxy;
pub mod saved_views;
pub mod sessions;
pub mod sign_in_attempts;
pub mod tags;
pub mod teams;
#[cfg(feature = "testing")]
pub mod testing;
pub mod users;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database};
pub use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, Statement, TransactionTrait};
use sea_orm_migration::MigratorTrait;

pub use error::StoreError;

/// Opens a connection pool for `url` (`sqlite://…` or `mysql://…`). Doesn't run migrations.
pub async fn connect(url: &str) -> Result<DatabaseConnection, StoreError> {
    if url.contains(":memory:") {
        return connect_in_memory().await;
    }
    Database::connect(pool_options(url)).await.map_err(StoreError::from)
}

fn pool_options(url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(url);
    options
        .sqlx_logging(false)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10));
    options
}

/// A fresh SQLite database in memory, for tests and trying the server out. It's a named database in shared-cache
/// mode, which lives while any connection to it is open. The pool may close its connection (sqlx does when a request
/// is cancelled mid-query) and open another, so a second connection that's never used holds the database open for
/// the rest of the process. The pool keeps to one connection, as shared-cache connections lock each other's tables.
async fn connect_in_memory() -> Result<DatabaseConnection, StoreError> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let url = format!(
        "sqlite:file:upheap-memory-{}-{n}?mode=memory&cache=shared",
        std::process::id()
    );

    let mut anchor = pool_options(&url);
    anchor
        .min_connections(1)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None);
    Box::leak(Box::new(Database::connect(anchor).await?));

    let mut options = pool_options(&url);
    options.min_connections(1).max_connections(1);
    Database::connect(options).await.map_err(StoreError::from)
}

/// Applies every pending migration and returns how many ran.
pub async fn run_migrations(db: &DatabaseConnection) -> Result<usize, StoreError> {
    let pending = migration::Migrator::get_pending_migrations(db).await?.len();
    tracing::info!(pending, "applying migrations");
    migration::Migrator::up(db, None).await?;
    tracing::info!(applied = pending, "migrations applied");
    Ok(pending)
}

/// Checks that the database answers.
pub async fn ping(db: &DatabaseConnection) -> Result<(), StoreError> {
    db.ping().await.map_err(StoreError::from)
}
