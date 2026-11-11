//! The turns migration on a database that already has questions: the open ones on closed items are withdrawn at the
//! item's close time, and the rest are left as they were.

use anyhow::{Context, Result};
use domain::ItemId;
use domain::question::QuestionState;
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, DbBackend};
use uuid::Uuid;

/// Migrations before the turns one.
const BEFORE_TURNS: u32 = 20;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

async fn insert_item(db: &DatabaseConnection, n: u128, key: &str, state: &str, closed_at: Option<i64>) -> Result<()> {
    // `key` is reserved on MySQL, which quotes it with backticks; SQLite and Postgres take double quotes.
    let column = match db.get_database_backend() {
        DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    let closed_at = closed_at.map_or("NULL".to_owned(), |at| at.to_string());
    run(
        db,
        &format!(
            "INSERT INTO items (id, heap_id, {column}, title, state, need, created_at, closed_at, version) \
             VALUES ('{}', '{}', '{key}', 'Item', '{state}', 'expected', 0, {closed_at}, 1)",
            id(n),
            id(2)
        ),
    )
    .await
}

async fn insert_question(db: &DatabaseConnection, n: u128, item: u128, number: u32, state: &str) -> Result<()> {
    let reply = if state == "answered" {
        "'[0]', 5000"
    } else {
        "NULL, NULL"
    };
    run(
        db,
        &format!(
            "INSERT INTO item_questions (id, item_id, number, kind, text, choices, many, state, asked_at, \
             reply_picked, replied_at, version) \
             VALUES ('{}', '{}', {number}, 'question', 'Which?', '[\"A\"]', false, '{state}', 100, {reply}, 1)",
            id(n),
            id(item)
        ),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn open_questions_on_closed_items_are_withdrawn_at_the_close() -> Result<()> {
    // Arrange: item 10 is done (closed at 7000) with an open question 30 and an answered one 31; item 11 is
    // won't do (closed at 8000) with an open question 32; item 12 is open with an open question 33.
    let db = store::testing::database_before_migration(BEFORE_TURNS).await?;
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
    insert_item(&db, 10, "k3x9a", "done", Some(7000)).await?;
    insert_item(&db, 11, "m4y8b", "wont_do", Some(8000)).await?;
    insert_item(&db, 12, "n5z7c", "open", None).await?;
    insert_question(&db, 30, 10, 1, "open").await?;
    insert_question(&db, 31, 10, 2, "answered").await?;
    insert_question(&db, 32, 11, 1, "open").await?;
    insert_question(&db, 33, 12, 1, "open").await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let questions = |item: u128| store::questions::list_questions(&db, ItemId::from_uuid(Uuid::from_u128(item)));
    let done = questions(10).await?;
    expect_that!(done.len(), eq(2));
    let withdrawn = &done[0];
    expect_that!(withdrawn.state(), eq(QuestionState::Withdrawn));
    let withdrawal = withdrawn.withdrawal.as_ref().context("withdrawal")?;
    expect_that!(withdrawal.withdrawn_at.as_millis(), eq(7000));
    expect_that!(withdrawal.withdrawn_by, none());
    expect_that!(done[1].state(), eq(QuestionState::Answered));
    let wont_do = questions(11).await?;
    expect_that!(wont_do[0].state(), eq(QuestionState::Withdrawn));
    expect_that!(
        wont_do[0].withdrawal.as_ref().map(|w| w.withdrawn_at.as_millis()),
        some(eq(8000))
    );
    expect_that!(questions(12).await?[0].state(), eq(QuestionState::Open));
    Ok(())
}
