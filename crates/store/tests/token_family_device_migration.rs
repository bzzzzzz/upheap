//! The token family device migration on a database that already has sign-ins: each reads back with no device name.

use anyhow::{Context, Result};
use domain::{Timestamp, UserId};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection};
use uuid::Uuid;

/// Migrations before the device name one.
const BEFORE_DEVICE_NAME: u32 = 13;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn an_existing_sign_in_has_no_device_name() -> Result<()> {
    // Arrange: Ada is signed in, with a refresh token that is still good.
    let db = store::testing::database_before_migration(BEFORE_DEVICE_NAME).await?;
    run(
        &db,
        &format!(
            "INSERT INTO users (id, email, display_name, created_at, version) \
             VALUES ('{}', 'ada@example.test', 'Ada', 0, 1)",
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO oauth_token_families (id, user_id, client_id, created_at) \
             VALUES ('{}', '{}', 'upheap-cli', 1000)",
            id(2),
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO oauth_tokens (token_hash, family_id, kind, created_at, expires_at) \
             VALUES ('{}', '{}', 'refresh', 1000, 9000)",
            "a".repeat(64),
            id(2)
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let now = Timestamp::from_millis(2000).context("timestamp")?;
    let live = store::oauth::list_live_token_families(&db, UserId::from_uuid(Uuid::from_u128(1)), now).await?;
    expect_that!(live.len(), eq(1));
    expect_that!(live[0].device_name, none());
    Ok(())
}
