//! Blockers: picking the item a blocked item waits for, the checks on it, and inherited priority.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

async fn titled(app: &TestApp, session: &str, heap: &str, title: &str) -> Result<Value> {
    app.new_item(session, heap, json!({ "title": title })).await
}

fn id(item: &Value) -> Result<&str> {
    item["id"].as_str().context("item has no id")
}

/// Sends a `PATCH` for `item` at its current version.
async fn patch(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<TestResponse> {
    app.patch(session, &format!("/api/v1/items/{}", id(item)?), &item["version"], body)
        .await
}

/// Blocks `item` on `blocker` and returns the saved item.
async fn block(app: &TestApp, session: &str, item: &Value, blocker: &Value) -> Result<Value> {
    let saved = patch(
        app,
        session,
        item,
        json!({ "state": "blocked", "blocked_by_item_id": id(blocker)? }),
    )
    .await?;
    anyhow::ensure!(saved.status == StatusCode::OK, "block failed: {}", saved.body);
    Ok(saved.body)
}

async fn ranked(app: &TestApp, session: &str, heap: &str) -> Result<Value> {
    let list = app
        .send(Method::GET, &format!("/api/v1/heaps/{heap}/items"), Some(session), None)
        .await?;
    anyhow::ensure!(list.status == StatusCode::OK, "list failed: {}", list.body);
    Ok(list.body)
}

#[gtest]
#[tokio::test]
async fn a_blocker_takes_on_the_priority_of_what_it_blocks() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    titled(&app, &session, &heap, "Tidy docs").await?;
    let blocker = titled(&app, &session, &heap, "Upgrade database").await?;
    let urgent = app
        .new_item(
            &session,
            &heap,
            json!({ "title": "Ship release", "due": { "date": "2020-01-01", "firm": true } }),
        )
        .await?;

    // Act
    let blocked = block(&app, &session, &urgent, &blocker).await?;
    let list = ranked(&app, &session, &heap).await?;

    // Assert
    expect_that!(blocked["state"], eq(&json!("blocked")));
    expect_that!(
        blocked["blocked_by"],
        eq(&json!({ "id": id(&blocker)?, "title": "Upgrade database", "state": "open" }))
    );
    expect_that!(list["ranked"][0]["item"]["title"], eq(&json!("Upgrade database")));
    expect_that!(list["ranked"][0]["contributors"][0]["factor"], eq(&json!("blocks")));
    expect_that!(
        list["ranked"][0]["contributors"][0]["label"],
        eq(&json!("Blocks “Ship release”"))
    );
    expect_that!(
        list["blocked"][0]["item"]["blocked_by"]["title"],
        eq(&json!("Upgrade database"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_block_that_would_close_a_cycle_is_refused() -> Result<()> {
    // Arrange: c waits for b, which waits for a.
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let a = titled(&app, &session, &heap, "A").await?;
    let b = titled(&app, &session, &heap, "B").await?;
    let c = titled(&app, &session, &heap, "C").await?;
    block(&app, &session, &b, &a).await?;
    block(&app, &session, &c, &b).await?;

    // Act
    let around = patch(
        &app,
        &session,
        &a,
        json!({ "state": "blocked", "blocked_by_item_id": id(&c)? }),
    )
    .await?;
    let itself = patch(
        &app,
        &session,
        &a,
        json!({ "state": "blocked", "blocked_by_item_id": id(&a)? }),
    )
    .await?;
    let unchanged = app
        .send(Method::GET, &format!("/api/v1/items/{}", id(&a)?), Some(&session), None)
        .await?;

    // Assert
    expect_that!(around.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(around.body["code"], eq(&json!("blocking_cycle")));
    expect_that!(around.body["field"], eq(&json!("blocked_by_item_id")));
    expect_that!(itself.body["code"], eq(&json!("blocking_cycle")));
    expect_that!(unchanged.body["state"], eq(&json!("open")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_blocker_must_be_an_open_item_of_the_same_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    let item = titled(&app, &session, &heap, "Waits").await?;
    let foreign = titled(&app, &other_session, &other_heap, "Elsewhere").await?;
    let finished = titled(&app, &session, &heap, "Finished").await?;
    let finished = patch(&app, &session, &finished, json!({ "state": "done" })).await?.body;

    // Act
    let block_on = |blocker: String| {
        patch(
            &app,
            &session,
            &item,
            json!({ "state": "blocked", "blocked_by_item_id": blocker }),
        )
    };
    let other_heap = block_on(id(&foreign)?.to_owned()).await?;
    let unknown = block_on("00000000-0000-7000-8000-000000000000".to_owned()).await?;
    let closed = block_on(id(&finished)?.to_owned()).await?;

    // Assert
    expect_that!(other_heap.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(other_heap.body["code"], eq(&json!("blocker_not_in_heap")));
    expect_that!(unknown.body["code"], eq(&json!("blocker_not_in_heap")));
    expect_that!(closed.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(closed.body["code"], eq(&json!("blocker_closed")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn only_a_blocked_item_can_have_a_blocker() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "First").await?;
    let item = titled(&app, &session, &heap, "Second").await?;
    let finished = titled(&app, &session, &heap, "Finished").await?;
    patch(&app, &session, &finished, json!({ "state": "done" })).await?;

    // Act
    let refused = patch(&app, &session, &item, json!({ "blocked_by_item_id": id(&blocker)? })).await?;
    // The item's state is the first problem, even with a blocker that would also be refused.
    let refused_closed = patch(&app, &session, &item, json!({ "blocked_by_item_id": id(&finished)? })).await?;
    let refused_itself = patch(&app, &session, &item, json!({ "blocked_by_item_id": id(&item)? })).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(refused.body["code"], eq(&json!("item_not_blocked")));
    expect_that!(refused_closed.body["code"], eq(&json!("item_not_blocked")));
    expect_that!(refused_itself.body["code"], eq(&json!("item_not_blocked")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn unblocking_drops_the_blocker_and_is_audited() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "First").await?;
    let item = titled(&app, &session, &heap, "Second").await?;
    let blocked = block(&app, &session, &item, &blocker).await?;

    // Act
    let unblocked = patch(&app, &session, &blocked, json!({ "state": "in_progress" })).await?;
    let entries = app.audit_entries(id(&item)?).await?;

    // Assert
    expect_that!(unblocked.status, eq(StatusCode::OK));
    expect_that!(unblocked.body["blocked_by"], eq(&Value::Null));
    // Entries written in the same millisecond have no defined order, so the order isn't checked.
    let diffs: Vec<&Value> = entries
        .iter()
        .filter(|(action, _)| action == "item_updated")
        .map(|(_, diff)| &diff["blocked_by"])
        .collect();
    expect_that!(
        diffs,
        unordered_elements_are![
            eq(&&json!({ "before": null, "after": id(&blocker)? })),
            eq(&&json!({ "before": id(&blocker)?, "after": null })),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sending_null_clears_the_blocker_and_keeps_the_item_blocked() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "First").await?;
    let item = titled(&app, &session, &heap, "Second").await?;
    let blocked = block(&app, &session, &item, &blocker).await?;

    // Act
    let cleared = patch(&app, &session, &blocked, json!({ "blocked_by_item_id": null })).await?;

    // Assert
    expect_that!(cleared.status, eq(StatusCode::OK));
    expect_that!(cleared.body["state"], eq(&json!("blocked")));
    expect_that!(cleared.body["blocked_by"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_blocker_still_shows_on_what_waits_for_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "First").await?;
    let item = titled(&app, &session, &heap, "Second").await?;
    block(&app, &session, &item, &blocker).await?;

    // Act
    let closed = patch(&app, &session, &blocker, json!({ "state": "done" })).await?;
    let list = ranked(&app, &session, &heap).await?;

    // Assert
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(
        list["blocked"][0]["item"]["blocked_by"],
        eq(&json!({ "id": id(&blocker)?, "title": "First", "state": "done" }))
    );
    Ok(())
}
