//! The runs migration on a database that already has turns, questions and audit entries: the new columns have their
//! defaults on existing rows, and nothing needs a backfill, since no run exists yet.

use anyhow::{Context, Result};
use domain::ItemId;
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection};
use uuid::Uuid;

/// Migrations before the runs one.
const BEFORE_RUNS: u32 = 22;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn existing_turns_and_questions_get_the_defaults_and_no_run() -> Result<()> {
    // Arrange: a team, a heap and an item with an open turn and an open question, before runs.
    let db = store::testing::database_before_migration(BEFORE_RUNS).await?;
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
    let key_column = match db.get_database_backend() {
        store::DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    run(
        &db,
        &format!(
            "INSERT INTO items (id, heap_id, {key_column}, title, state, need, created_at, version) \
             VALUES ('{}', '{}', 'k3x9a', 'Item', 'open', 'expected', 0, 1)",
            id(10),
            id(2)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO item_turns (id, item_id, reach, goal, source, opened_at) \
             VALUES ('{}', '{}', 'one', 'with_context', 'go_ahead', 100)",
            id(20),
            id(10)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO item_questions (id, item_id, number, kind, text, choices, many, state, asked_at, version) \
             VALUES ('{}', '{}', 1, 'question', 'Which?', '[\"A\"]', false, 'open', 100, 1)",
            id(30),
            id(10)
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let item = ItemId::from_uuid(Uuid::from_u128(10));
    let turn = store::turns::find_open_turn(&db, item).await?.context("turn")?;
    expect_that!(turn.failed_runs, eq(0));
    expect_that!(turn.is_stuck(), eq(false));
    let questions = store::questions::list_questions(&db, item).await?;
    expect_that!(questions.len(), eq(1));
    expect_that!(questions[0].run_id, none());
    let runs =
        store::runs::list_unended_runs_in_heaps(&db, &[domain::HeapId::from_uuid(Uuid::from_u128(2))], false).await?;
    expect_that!(runs, is_empty());
    Ok(())
}
