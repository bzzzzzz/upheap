//! Test databases for the store and API test suites. Only built with the `testing` feature.
//!
//! The suites run on in-memory SQLite by default. Set `TEST_DATABASE_URL` to a MySQL server URL without a
//! database name (`mysql://root:root@127.0.0.1:3306`) to run them on MySQL instead: each call then creates its own
//! empty database on that server, so tests stay independent and can run in parallel.

use sea_orm::ConnectionTrait;
use uuid::Uuid;

use crate::{DatabaseConnection, StoreError};

/// The environment variable that points the test suites at a MySQL server.
pub const TEST_DATABASE_URL: &str = "TEST_DATABASE_URL";

/// Opens a new, empty test database with every migration applied.
///
/// # Errors
///
/// Fails if the server can't be reached, the database can't be created, or a migration fails.
pub async fn fresh_database() -> Result<DatabaseConnection, StoreError> {
    let db = match std::env::var(TEST_DATABASE_URL) {
        Ok(server) if !server.is_empty() => fresh_mysql_database(server.trim_end_matches('/')).await?,
        _ => crate::connect("sqlite::memory:").await?,
    };
    crate::run_migrations(&db).await?;
    Ok(db)
}

/// Creates `upheap_test_<uuid>` on `server` and connects to it.
///
/// The databases are left behind when the test ends; the server is expected to be a throwaway container.
async fn fresh_mysql_database(server: &str) -> Result<DatabaseConnection, StoreError> {
    let name = format!("upheap_test_{}", Uuid::now_v7().simple());
    let admin = crate::connect(server).await?;
    admin
        .execute_unprepared(&format!(
            "CREATE DATABASE `{name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci"
        ))
        .await?;
    admin.close().await?;
    crate::connect(&format!("{server}/{name}")).await
}
