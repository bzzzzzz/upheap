//! Test databases for the store and API test suites. Only built with the `testing` feature.
//!
//! The suites run on in-memory SQLite by default. Set `TEST_DATABASE_URL` to a MySQL or Postgres server URL
//! without a database name (`mysql://root:root@127.0.0.1:3306`, `postgres://postgres:postgres@127.0.0.1:5432`) to
//! run them there instead: each call then creates its own empty database on that server, so tests stay independent
//! and can run in parallel.

use sea_orm::ConnectionTrait;
use sea_orm_migration::MigratorTrait;
use uuid::Uuid;

use crate::{DatabaseConnection, StoreError};

/// The environment variable that points the test suites at a MySQL or Postgres server.
pub const TEST_DATABASE_URL: &str = "TEST_DATABASE_URL";

/// Opens a new, empty test database with every migration applied.
///
/// # Errors
///
/// Fails if the server can't be reached, the database can't be created, or a migration fails.
pub async fn fresh_database() -> Result<DatabaseConnection, StoreError> {
    let db = match std::env::var(TEST_DATABASE_URL) {
        Ok(server) if !server.is_empty() => fresh_server_database(server.trim_end_matches('/')).await?,
        _ => crate::connect("sqlite::memory:", 1).await?,
    };
    crate::run_migrations(&db).await?;
    Ok(db)
}

/// Opens a new, empty test database with only the first `migrations` migrations applied, for testing a later
/// migration on data written before it. [`crate::run_migrations`] applies the rest.
///
/// # Errors
///
/// As [`fresh_database`].
pub async fn database_before_migration(migrations: u32) -> Result<DatabaseConnection, StoreError> {
    let db = match std::env::var(TEST_DATABASE_URL) {
        Ok(server) if !server.is_empty() => fresh_server_database(server.trim_end_matches('/')).await?,
        _ => crate::connect("sqlite::memory:", 1).await?,
    };
    crate::migration::Migrator::up(&db, Some(migrations)).await?;
    Ok(db)
}

/// Connections each test database keeps at most: enough for the tests that race several requests. Pools open
/// connections only as they're needed, so most tests use one or two.
const TEST_POOL_SIZE: u32 = 10;

/// Creates `upheap_test_<uuid>` on `server` (MySQL or Postgres) and connects to it.
///
/// The databases are left behind when the test ends; the server is expected to be a throwaway container.
async fn fresh_server_database(server: &str) -> Result<DatabaseConnection, StoreError> {
    let name = format!("upheap_test_{}", Uuid::now_v7().simple());
    let create = if server.starts_with("mysql:") {
        format!("CREATE DATABASE `{name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci")
    } else {
        format!("CREATE DATABASE \"{name}\"")
    };
    let admin = crate::connect(server, 1).await?;
    admin.execute_unprepared(&create).await?;
    admin.close().await?;
    crate::connect(&format!("{server}/{name}"), TEST_POOL_SIZE).await
}
