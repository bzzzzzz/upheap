//! The item closer migration on a database that already has a closed item: it still loads, closed by nobody known.

use anyhow::{Context, Result};
use domain::ItemId;
use domain::item::StateKind;
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, DbBackend};
use uuid::Uuid;

/// Migrations before the item closer one.
const BEFORE_ITEM_CLOSER: u32 = 11;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn an_item_closed_before_the_migration_has_no_closer() -> Result<()> {
    // Arrange
    let db = store::testing::database_before_migration(BEFORE_ITEM_CLOSER).await?;
    run(
        &db,
        &format!(
            "INSERT INTO teams (id, slug, name, created_at, version) VALUES ('{}', 'infra', 'Infra', 0, 1)",
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, created_at, version) \
             VALUES ('{}', '{}', 'backlog', 'Backlog', 10, 10, 10, 0, 1)",
            id(2),
            id(1)
        ),
    )
    .await?;
    // `key` is reserved on MySQL, which quotes it with backticks; SQLite and Postgres take double quotes.
    let key = match db.get_database_backend() {
        DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    run(
        &db,
        &format!(
            "INSERT INTO items (id, heap_id, {key}, title, state, need, created_at, closed_at, version) \
             VALUES ('{}', '{}', 'k3x9a', 'Item', 'done', 'expected', 0, 1000, 2)",
            id(10),
            id(2)
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let item = store::items::find_item(&db, ItemId::from_uuid(Uuid::from_u128(10)))
        .await?
        .context("item 10")?;
    expect_that!(item.state.kind(), eq(StateKind::Done));
    expect_that!(item.state.closed_by(), none());
    Ok(())
}
