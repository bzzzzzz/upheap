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
pub mod sessions;
pub mod tags;
pub mod teams;
pub mod users;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database};
pub use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, Statement, TransactionTrait};
use sea_orm_migration::MigratorTrait;

pub use error::StoreError;

/// Opens a connection pool for `url` (`sqlite://…` or `mysql://…`). Doesn't run migrations.
pub async fn connect(url: &str) -> Result<DatabaseConnection, StoreError> {
    let mut options = ConnectOptions::new(url);
    options
        .sqlx_logging(false)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10));
    if url.contains(":memory:") {
        // Every SQLite in-memory connection is a separate, empty database, so the pool must hold exactly one.
        options.min_connections(1).max_connections(1);
    }
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
