//! The item closer migration on a database that already has closed items: each gets the actor of the update entry
//! written at its close time, and an item without one keeps no closer.

use anyhow::{Context, Result};
use domain::item::StateKind;
use domain::{ItemId, UserId};
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

async fn insert_closed_item(db: &DatabaseConnection, n: u128, key: &str, closed_at: i64) -> Result<()> {
    // `key` is reserved on MySQL, which quotes it with backticks; SQLite and Postgres take double quotes.
    let column = match db.get_database_backend() {
        DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    run(
        db,
        &format!(
            "INSERT INTO items (id, heap_id, {column}, title, state, need, created_at, closed_at, version) \
             VALUES ('{}', '{}', '{key}', 'Item', 'done', 'expected', 0, {closed_at}, 2)",
            id(n),
            id(2)
        ),
    )
    .await
}

async fn insert_update_entry(db: &DatabaseConnection, n: u128, actor: u128, item: u128, at: i64) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO audit_log (id, team_id, heap_id, actor_user_id, action, entity_type, entity_id, diff, created_at) \
             VALUES ('{}', '{}', '{}', '{}', 'item_updated', 'item', '{}', '{{}}', {at})",
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
async fn closed_items_get_the_closer_from_the_entry_at_their_close_time() -> Result<()> {
    // Arrange: Bob edits item 10 and Ada closes it at 1000; item 11, closed at 2000, has only an earlier edit.
    let db = store::testing::database_before_migration(BEFORE_ITEM_CLOSER).await?;
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
    insert_closed_item(&db, 10, "k3x9a", 1000).await?;
    insert_closed_item(&db, 11, "m4y8b", 2000).await?;
    insert_update_entry(&db, 20, 6, 10, 500).await?;
    insert_update_entry(&db, 21, 5, 10, 1000).await?;
    insert_update_entry(&db, 22, 6, 11, 1999).await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let find = |n: u128| store::items::find_item(&db, ItemId::from_uuid(Uuid::from_u128(n)));
    let traced = find(10).await?.context("item 10")?;
    let untraced = find(11).await?.context("item 11")?;
    expect_that!(traced.state.kind(), eq(StateKind::Done));
    expect_that!(
        traced.state.closed_by(),
        some(eq(UserId::from_uuid(Uuid::from_u128(5))))
    );
    expect_that!(untraced.state.kind(), eq(StateKind::Done));
    expect_that!(untraced.state.closed_by(), none());
    Ok(())
}
