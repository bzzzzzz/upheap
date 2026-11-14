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

/// The item with ID `item_id` as it is now.
async fn get(app: &TestApp, session: &str, item_id: &str) -> Result<Value> {
    let found = app
        .send(Method::GET, &app.item_url(item_id).await?, Some(session), None)
        .await?;
    anyhow::ensure!(found.status == StatusCode::OK, "get failed: {}", found.body);
    Ok(found.body)
}

/// What waits for the caller.
async fn waiting(app: &TestApp, session: &str) -> Result<Value> {
    let found = app.send(Method::GET, "/api/v1/me/waiting", Some(session), None).await?;
    anyhow::ensure!(found.status == StatusCode::OK, "waiting failed: {}", found.body);
    Ok(found.body)
}

/// The history of the item with ID `entity` in `heap`, newest first.
async fn history(app: &TestApp, session: &str, heap: &str, entity: &str) -> Result<Value> {
    let audit = app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={entity}", app.heap_url(heap).await?),
            Some(session),
            None,
        )
        .await?;
    anyhow::ensure!(audit.status == StatusCode::OK, "history failed: {}", audit.body);
    Ok(audit.body)
}

fn id(item: &Value) -> Result<&str> {
    item["id"].as_str().context("item has no id")
}

/// Sends a `PATCH` for `item` at its current version.
async fn patch(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<TestResponse> {
    app.patch(session, &app.item_url(id(item)?).await?, &item["version"], body)
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

/// The ranked list with blocked items shown, which are hidden by default.
async fn ranked(app: &TestApp, session: &str, heap: &str) -> Result<Value> {
    let list = app
        .send(
            Method::GET,
            &format!("{}/items?blocked=shown", app.heap_url(&heap).await?),
            Some(session),
            None,
        )
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
        eq(
            &json!({ "id": id(&blocker)?, "key": blocker["key"], "title": "Upgrade database", "kind": "note", "state": "open" })
        )
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
        .send(Method::GET, &app.item_url(id(&a)?).await?, Some(&session), None)
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
async fn closing_a_blocker_as_done_frees_what_it_blocks() -> Result<()> {
    // Arrange: two items wait for First; a third waits for another item.
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "First").await?;
    let second = titled(&app, &session, &heap, "Second").await?;
    let third = titled(&app, &session, &heap, "Third").await?;
    let other_blocker = titled(&app, &session, &heap, "Other").await?;
    let other = titled(&app, &session, &heap, "Fourth").await?;
    let second = block(&app, &session, &second, &blocker).await?;
    block(&app, &session, &third, &blocker).await?;
    let other = block(&app, &session, &other, &other_blocker).await?;

    // Act
    let closed = patch(&app, &session, &blocker, json!({ "state": "done" })).await?;
    let freed = get(&app, &session, id(&second)?).await?;
    let third = get(&app, &session, id(&third)?).await?;
    let other_after = get(&app, &session, id(&other)?).await?;
    let log = history(&app, &session, &heap, id(&second)?).await?;
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(freed["state"], eq(&json!("open")));
    expect_that!(freed["blocked_by"], eq(&Value::Null));
    expect_that!(freed["blocker_gone"], eq(&Value::Null));
    expect_that!(freed["waits_for_you"], eq(&json!(false)));
    // Freeing it is a write, so an edit made from the blocked version is refused.
    expect_that!(
        freed["version"],
        eq(&json!(second["version"].as_u64().unwrap_or(0) + 1))
    );
    expect_that!(third["state"], eq(&json!("open")));
    expect_that!(other_after["state"], eq(&json!("blocked")));
    expect_that!(other_after["version"], eq(&other["version"]));
    expect_that!(log["entries"][0]["action"], eq(&json!("item_updated")));
    expect_that!(log["entries"][0]["diff"]["blocker_done"], eq(&json!(id(&blocker)?)));
    expect_that!(
        log["entries"][0]["changes"],
        eq(&json!([
            { "field": "blocker_done", "label": "Blocker done", "before": null, "after": "First", "text_diff": null },
            { "field": "state", "label": "State", "before": "Blocked", "after": "Open", "text_diff": null },
            { "field": "blocked_by", "label": "Blocked by", "before": "First", "after": null, "text_diff": null },
        ]))
    );
    expect_that!(waiting["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn closing_a_blocker_as_wont_do_makes_what_it_blocks_wait_for_the_person() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "Pick a host").await?;
    let item = titled(&app, &session, &heap, "Ship the feed").await?;
    let item = block(&app, &session, &item, &blocker).await?;

    // Act
    let closed = patch(&app, &session, &blocker, json!({ "state": "wont_do" })).await?;
    let after = get(&app, &session, id(&item)?).await?;
    let list = ranked(&app, &session, &heap).await?;
    let waiting = waiting(&app, &session).await?;

    // Assert: nothing is written to the item; it reads as waiting.
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(after["state"], eq(&json!("blocked")));
    expect_that!(after["version"], eq(&item["version"]));
    expect_that!(
        after["blocked_by"],
        eq(
            &json!({ "id": id(&blocker)?, "key": blocker["key"], "title": "Pick a host", "kind": "note", "state": "wont_do" })
        )
    );
    expect_that!(
        after["blocker_gone"],
        eq(&json!({ "how": "wont_do", "moved_to": null }))
    );
    expect_that!(after["waits_for_you"], eq(&json!(true)));
    expect_that!(list["blocked"][0]["item"]["waits_for_you"], eq(&json!(true)));
    expect_that!(list["blocked"][0]["item"]["blocker_gone"]["how"], eq(&json!("wont_do")));
    expect_that!(waiting["count"], eq(&json!(1)));
    expect_that!(
        waiting["rows"][0],
        eq(&json!({
            "kind": "blocker_gone",
            "item": waiting["rows"][0]["item"].clone(),
            "heap": waiting["rows"][0]["heap"].clone(),
            "question": null,
            "stuck": null,
            "blocker": {
                "item": { "id": id(&blocker)?, "key": blocker["key"], "title": "Pick a host", "kind": "note", "state": "wont_do" },
                "gone": { "how": "wont_do", "moved_to": null },
                "allowed_actions": ["change_state"],
            },
            "step_title": null,
        }))
    );
    expect_that!(waiting["rows"][0]["item"]["id"], eq(&json!(id(&item)?)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_to_come_whose_blocker_is_gone_waits_after_the_ranked_rows() -> Result<()> {
    // Arrange: a reminder for later and a plain item, both blocked by the same item.
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "Pick a host").await?;
    let reminder = app
        .new_item(
            &session,
            &heap,
            json!({ "title": "Renew the domain", "kind": "reminder", "due": { "date": "2099-01-20", "firm": false } }),
        )
        .await?;
    let reminder = block(&app, &session, &reminder, &blocker).await?;
    let item = titled(&app, &session, &heap, "Ship the feed").await?;
    let item = block(&app, &session, &item, &blocker).await?;

    // Act
    let closed = patch(&app, &session, &blocker, json!({ "state": "wont_do" })).await?;
    let waiting = waiting(&app, &session).await?;

    // Assert: both wait, the reminder after the ranked item, and both count.
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(waiting["count"], eq(&json!(2)));
    expect_that!(waiting["rows"][0]["kind"], eq(&json!("blocker_gone")));
    expect_that!(waiting["rows"][0]["item"]["id"], eq(&json!(id(&item)?)));
    expect_that!(waiting["rows"][1]["kind"], eq(&json!("blocker_gone")));
    expect_that!(waiting["rows"][1]["item"]["id"], eq(&json!(id(&reminder)?)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn unblocking_an_item_whose_blocker_is_gone_stops_it_waiting() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "Pick a host").await?;
    let item = titled(&app, &session, &heap, "Ship the feed").await?;
    block(&app, &session, &item, &blocker).await?;
    patch(&app, &session, &blocker, json!({ "state": "wont_do" })).await?;
    let current = get(&app, &session, id(&item)?).await?;

    // Act
    let unblocked = patch(&app, &session, &current, json!({ "state": "open" })).await?;
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(unblocked.status, eq(StatusCode::OK));
    expect_that!(unblocked.body["state"], eq(&json!("open")));
    expect_that!(unblocked.body["blocker_gone"], eq(&Value::Null));
    expect_that!(unblocked.body["waits_for_you"], eq(&json!(false)));
    expect_that!(waiting["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reopening_a_blocker_leaves_what_it_freed_open_and_holds_what_it_left_waiting() -> Result<()> {
    // Arrange: Done frees Freed; Refused leaves Waiting waiting.
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let done = titled(&app, &session, &heap, "Done").await?;
    let refused = titled(&app, &session, &heap, "Refused").await?;
    let freed = titled(&app, &session, &heap, "Freed").await?;
    let left = titled(&app, &session, &heap, "Waiting").await?;
    block(&app, &session, &freed, &done).await?;
    block(&app, &session, &left, &refused).await?;
    let done = patch(&app, &session, &done, json!({ "state": "done" })).await?.body;
    let refused = patch(&app, &session, &refused, json!({ "state": "wont_do" }))
        .await?
        .body;

    // Act
    let reopened_done = patch(&app, &session, &done, json!({ "state": "open" })).await?;
    let reopened_refused = patch(&app, &session, &refused, json!({ "state": "open" })).await?;
    let freed = get(&app, &session, id(&freed)?).await?;
    let left = get(&app, &session, id(&left)?).await?;
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(reopened_done.status, eq(StatusCode::OK));
    expect_that!(reopened_refused.status, eq(StatusCode::OK));
    expect_that!(freed["state"], eq(&json!("open")));
    expect_that!(left["state"], eq(&json!("blocked")));
    expect_that!(left["blocked_by"]["state"], eq(&json!("open")));
    expect_that!(left["blocker_gone"], eq(&Value::Null));
    expect_that!(left["waits_for_you"], eq(&json!(false)));
    expect_that!(waiting["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reason_round_trips_and_is_replaced_or_cleared_while_blocked() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = titled(&app, &session, &heap, "Second").await?;
    expect_that!(item["blocked_reason"], eq(&Value::Null));

    // Act
    let blocked = patch(
        &app,
        &session,
        &item,
        json!({ "state": "blocked", "blocked_reason": "  Waits for the vendor's quote " }),
    )
    .await?;
    let read = get(&app, &session, id(&item)?).await?;
    let replaced = patch(
        &app,
        &session,
        &blocked.body,
        json!({ "blocked_reason": "Waits for legal" }),
    )
    .await?;
    let cleared = patch(&app, &session, &replaced.body, json!({ "blocked_reason": null })).await?;
    let log = history(&app, &session, &heap, id(&item)?).await?;

    // Assert
    expect_that!(blocked.status, eq(StatusCode::OK));
    expect_that!(
        blocked.body["blocked_reason"],
        eq(&json!("Waits for the vendor's quote"))
    );
    expect_that!(read["blocked_reason"], eq(&json!("Waits for the vendor's quote")));
    expect_that!(replaced.body["blocked_reason"], eq(&json!("Waits for legal")));
    expect_that!(cleared.body["state"], eq(&json!("blocked")));
    expect_that!(cleared.body["blocked_reason"], eq(&Value::Null));
    expect_that!(
        log["entries"][0]["changes"],
        eq(
            &json!([{ "field": "blocked_reason", "label": "Blocked because", "before": "Waits for legal", "after": null, "text_diff": null }])
        )
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bad_reason_is_refused_and_so_is_one_on_an_item_that_isnt_blocked() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = titled(&app, &session, &heap, "Second").await?;

    // Act
    let blank = patch(
        &app,
        &session,
        &item,
        json!({ "state": "blocked", "blocked_reason": "  " }),
    )
    .await?;
    let long = patch(
        &app,
        &session,
        &item,
        json!({ "state": "blocked", "blocked_reason": "x".repeat(301) }),
    )
    .await?;
    let not_blocked = patch(&app, &session, &item, json!({ "blocked_reason": "x" })).await?;
    let clearing = patch(&app, &session, &item, json!({ "blocked_reason": null })).await?;

    // Assert
    expect_that!(blank.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(blank.body["field"], eq(&json!("blocked_reason")));
    expect_that!(long.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(long.body["field"], eq(&json!("blocked_reason")));
    expect_that!(not_blocked.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(not_blocked.body["code"], eq(&json!("item_not_blocked")));
    expect_that!(clearing.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn leaving_blocked_clears_the_reason_and_a_blocker_done_frees_it_too() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = titled(&app, &session, &heap, "First").await?;
    let waits = titled(&app, &session, &heap, "Second").await?;
    let started = titled(&app, &session, &heap, "Third").await?;
    let waits = patch(
        &app,
        &session,
        &waits,
        json!({ "state": "blocked", "blocked_by_item_id": id(&blocker)?, "blocked_reason": "And the vendor" }),
    )
    .await?
    .body;
    let started = patch(
        &app,
        &session,
        &started,
        json!({ "state": "blocked", "blocked_reason": "Waits" }),
    )
    .await?
    .body;

    // Act
    let start = patch(&app, &session, &started, json!({ "state": "in_progress" })).await?;
    let closed = patch(&app, &session, &blocker, json!({ "state": "done" })).await?;
    let freed = get(&app, &session, id(&waits)?).await?;
    let log = history(&app, &session, &heap, id(&waits)?).await?;

    // Assert
    expect_that!(start.body["blocked_reason"], eq(&Value::Null));
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(freed["state"], eq(&json!("open")));
    expect_that!(freed["blocked_reason"], eq(&Value::Null));
    expect_that!(
        log["entries"][0]["diff"]["blocked_reason"],
        eq(&json!({ "before": "And the vendor", "after": null }))
    );
    Ok(())
}
