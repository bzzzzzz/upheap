//! The kind and stage migration on a database that already has heaps and items.

use anyhow::{Context, Result};
use domain::ItemId;
use domain::item::{Kind, Stage};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, DbBackend};
use uuid::Uuid;

/// Migrations before the kind and stage one.
const BEFORE_KIND: u32 = 14;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn existing_items_become_notes_as_written() -> Result<()> {
    // Arrange
    let db = store::testing::database_before_migration(BEFORE_KIND).await?;
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
            "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, need_weight, \
             created_at, version) VALUES ('{}', '{}', 'backlog', 'Backlog', 10, 10, 10, 10, 0, 1)",
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
            "INSERT INTO items (id, heap_id, {key}, title, state, need, created_at, version) \
             VALUES ('{}', '{}', 'k3x9a', 'Item', 'open', 'expected', 0, 1)",
            id(3),
            id(2)
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let item = store::items::find_item(&db, ItemId::from_uuid(Uuid::from_u128(3)))
        .await?
        .context("item")?;
    expect_that!(item.kind, eq(Kind::Note));
    expect_that!(item.stage, eq(Stage::AsWritten));
    Ok(())
}
