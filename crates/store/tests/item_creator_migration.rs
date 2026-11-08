//! The item creator migration on a database that already has items: each gets the actor of its `item_created`
//! audit entry, and an item without one keeps no creator.

use anyhow::{Context, Result};
use domain::{ItemId, UserId};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, DbBackend};
use uuid::Uuid;

/// Migrations before the item creator one.
const BEFORE_ITEM_CREATOR: u32 = 10;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

async fn insert_item(db: &DatabaseConnection, n: u128, key: &str) -> Result<()> {
    // `key` is reserved on MySQL, which quotes it with backticks; SQLite and Postgres take double quotes.
    let column = match db.get_database_backend() {
        DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    run(
        db,
        &format!(
            "INSERT INTO items (id, heap_id, {column}, title, state, need, created_at, version) \
             VALUES ('{}', '{}', '{key}', 'Item', 'open', 'expected', 0, 1)",
            id(n),
            id(2)
        ),
    )
    .await
}

async fn insert_audit_entry(db: &DatabaseConnection, n: u128, action: &str, actor: u128, item: u128) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO audit_log (id, team_id, heap_id, actor_user_id, action, entity_type, entity_id, diff, created_at) \
             VALUES ('{}', '{}', '{}', '{}', '{action}', 'item', '{}', '{{}}', 0)",
            id(n),
            id(1),
            id(2),
            id(actor),
            id(item)
        ),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn existing_items_get_the_creator_from_their_creation_entry() -> Result<()> {
    // Arrange: Ada adds item 10 and Bob edits it; item 11 has no creation entry.
    let db = store::testing::database_before_migration(BEFORE_ITEM_CREATOR).await?;
    for (n, name) in [(5, "Ada"), (6, "Bob")] {
        run(
            &db,
            &format!(
                "INSERT INTO users (id, email, display_name, created_at, version) \
                 VALUES ('{}', '{name}@example.test', '{name}', 0, 1)",
                id(n)
            ),
        )
        .await?;
    }
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
    insert_item(&db, 10, "k3x9a").await?;
    insert_item(&db, 11, "m4y8b").await?;
    insert_audit_entry(&db, 20, "item_updated", 6, 10).await?;
    insert_audit_entry(&db, 21, "item_created", 5, 10).await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let find = |n: u128| store::items::find_item(&db, ItemId::from_uuid(Uuid::from_u128(n)));
    let traced = find(10).await?.context("item 10")?;
    let untraced = find(11).await?.context("item 11")?;
    expect_that!(traced.created_by, some(eq(UserId::from_uuid(Uuid::from_u128(5)))));
    expect_that!(untraced.created_by, none());
    Ok(())
}
