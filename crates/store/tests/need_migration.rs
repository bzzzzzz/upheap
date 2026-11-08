//! The need migration on a database that already has heaps and items.

use anyhow::{Context, Result};
use domain::item::Need;
use domain::{HeapId, ItemId};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection};
use uuid::Uuid;

/// Migrations before the need one.
const BEFORE_NEED: u32 = 9;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn existing_items_become_expected_and_heaps_get_the_default_weight() -> Result<()> {
    // Arrange
    let db = store::testing::database_before_migration(BEFORE_NEED).await?;
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
    run(
        &db,
        &format!(
            "INSERT INTO items (id, heap_id, `key`, title, state, created_at, version) \
             VALUES ('{}', '{}', 'k3x9a', 'Item', 'open', 0, 1)",
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
    let heap = store::heaps::find_heap(&db, HeapId::from_uuid(Uuid::from_u128(2)))
        .await?
        .context("heap")?;
    expect_that!(item.need, eq(Need::Expected));
    expect_that!(heap.policy.need_weight, eq(10));
    Ok(())
}
