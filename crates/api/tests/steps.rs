//! A task's steps: adding, ticking, dropping, editing, and the rules around them.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

/// Registers a user and makes a task in their heap. Returns the app, the session, the heap, and the task's key.
async fn task() -> Result<(TestApp, String, String, String)> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let note = app
        .new_item(&session, &heap, json!({ "title": "Rotate the certs" }))
        .await?;
    let key = note["key"].as_str().context("no key")?.to_owned();
    let made = app
        .patch(
            &session,
            &app.item_url(note["id"].as_str().context("no id")?).await?,
            &note["version"],
            json!({ "kind": "task" }),
        )
        .await?;
    anyhow::ensure!(made.status == StatusCode::OK, "make task failed: {}", made.body);
    Ok((app, session, heap, key))
}

fn item_path(heap: &str, key: &str, app: &TestApp) -> impl std::future::Future<Output = Result<String>> {
    let heap = heap.to_owned();
    let key = key.to_owned();
    async move { Ok(format!("{}/items/{key}", app.heap_url(&heap).await?)) }
}

async fn add(app: &TestApp, session: &str, heap: &str, key: &str, body: Value) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/steps", item_path(heap, key, app).await?),
        Some(session),
        Some(body),
    )
    .await
}

async fn edit(
    app: &TestApp,
    session: &str,
    heap: &str,
    key: &str,
    step: &str,
    version: &Value,
    body: Value,
) -> Result<TestResponse> {
    app.patch(
        session,
        &format!("{}/steps/{step}", item_path(heap, key, app).await?),
        version,
        body,
    )
    .await
}

async fn get(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<Value> {
    let fetched = app
        .send(Method::GET, &item_path(heap, key, app).await?, Some(session), None)
        .await?;
    Ok(fetched.body)
}

/// The numbers of the item's steps, in order.
fn numbers(item: &Value) -> Vec<u64> {
    item["steps"]
        .as_array()
        .map(|steps| steps.iter().filter_map(|step| step["number"].as_u64()).collect())
        .unwrap_or_default()
}

/// The first step's `version`, as the API returns it.
fn step_version(item: &Value, number: u64) -> Value {
    item["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["number"] == json!(number)))
        .map_or(Value::Null, |step| step["version"].clone())
}

fn actions(value: &Value) -> Vec<&str> {
    value["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn steps_are_numbered_in_the_order_they_were_added_and_leave_the_items_version_alone() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    let before = get(&app, &session, &heap, &key).await?;

    // Act
    let first = add(&app, &session, &heap, &key, json!({ "title": "Inventory the certs" })).await?;
    let second = add(
        &app,
        &session,
        &heap,
        &key,
        json!({ "title": "Run in staging", "context": "Blue stack" }),
    )
    .await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::CREATED));
    expect_that!(numbers(&first.body), elements_are![eq(&1)]);
    expect_that!(numbers(&second.body), elements_are![eq(&1), eq(&2)]);
    expect_that!(second.body["steps"][1]["title"], eq(&json!("Run in staging")));
    expect_that!(second.body["steps"][1]["context"], eq(&json!("Blue stack")));
    expect_that!(second.body["steps"][0]["context"], eq(&Value::Null));
    expect_that!(second.body["steps"][1]["state"], eq(&json!("open")));
    expect_that!(second.body["steps"][1]["version"], eq(&json!(1)));
    expect_that!(second.body["steps"][1]["allowed_actions"], eq(&json!(["edit"])));
    expect_that!(second.body["step_progress"], eq(&json!({ "ticked": 0, "total": 2 })));
    expect_that!(second.body["version"], eq(&before["version"]));
    expect_that!(actions(&second.body), contains(eq(&"add_step")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_step_added_after_another_goes_right_behind_it() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    add(&app, &session, &heap, &key, json!({ "title": "Two" })).await?;

    // Act
    let third = add(&app, &session, &heap, &key, json!({ "title": "Three", "after": 1 })).await?;

    // Assert
    expect_that!(numbers(&third.body), elements_are![eq(&1), eq(&3), eq(&2)]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn ticking_a_step_bumps_its_version_and_not_the_items() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    let added = add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    add(&app, &session, &heap, &key, json!({ "title": "Two" })).await?;

    // Act
    let ticked = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Assert
    expect_that!(ticked.status, eq(StatusCode::OK));
    expect_that!(ticked.body["steps"][0]["state"], eq(&json!("ticked")));
    expect_that!(ticked.body["steps"][0]["version"], eq(&json!(2)));
    expect_that!(ticked.body["steps"][1]["version"], eq(&json!(1)));
    expect_that!(ticked.body["step_progress"], eq(&json!({ "ticked": 1, "total": 2 })));
    expect_that!(ticked.body["version"], eq(&added.body["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn editing_the_item_after_ticking_a_step_does_not_conflict() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    let added = add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Act
    let renamed = app
        .patch(
            &session,
            &item_path(&heap, &key, &app).await?,
            &added.body["version"],
            json!({ "title": "Rotate all the certs" }),
        )
        .await?;

    // Assert
    expect_that!(renamed.status, eq(StatusCode::OK));
    expect_that!(renamed.body["steps"][0]["state"], eq(&json!("ticked")));
    expect_that!(renamed.body["steps"][0]["version"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_dropped_step_stays_but_leaves_the_count_and_can_be_restored() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    add(&app, &session, &heap, &key, json!({ "title": "Two" })).await?;
    edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Act
    let dropped = edit(
        &app,
        &session,
        &heap,
        &key,
        "2",
        &json!(1),
        json!({ "state": "dropped" }),
    )
    .await?;
    let restored = edit(&app, &session, &heap, &key, "2", &json!(2), json!({ "state": "open" })).await?;

    // Assert
    expect_that!(dropped.body["steps"][1]["state"], eq(&json!("dropped")));
    expect_that!(dropped.body["step_progress"], eq(&json!({ "ticked": 1, "total": 1 })));
    expect_that!(restored.body["steps"][1]["state"], eq(&json!("open")));
    expect_that!(restored.body["step_progress"], eq(&json!({ "ticked": 1, "total": 2 })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_whose_steps_are_all_dropped_has_no_progress() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;

    // Act
    let dropped = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "dropped" }),
    )
    .await?;

    // Assert
    expect_that!(dropped.body["step_progress"], eq(&Value::Null));
    expect_that!(dropped.body["steps"][0]["state"], eq(&json!("dropped")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_step_dropped_can_go_straight_to_ticked() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "dropped" }),
    )
    .await?;

    // Act
    let ticked = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(2),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Assert
    expect_that!(ticked.status, eq(StatusCode::OK));
    expect_that!(ticked.body["steps"][0]["state"], eq(&json!("ticked")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn title_and_context_are_edited_and_null_clears_the_context() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;

    // Act
    let edited = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "title": "  Uno  ", "context": "Some background" }),
    )
    .await?;
    let cleared = edit(&app, &session, &heap, &key, "1", &json!(2), json!({ "context": null })).await?;
    let blank = edit(&app, &session, &heap, &key, "1", &json!(3), json!({ "context": "  " })).await?;

    // Assert
    expect_that!(edited.body["steps"][0]["title"], eq(&json!("Uno")));
    expect_that!(edited.body["steps"][0]["context"], eq(&json!("Some background")));
    expect_that!(cleared.body["steps"][0]["context"], eq(&Value::Null));
    expect_that!(cleared.body["steps"][0]["version"], eq(&json!(3)));
    // Clearing a context that is already cleared changes nothing.
    expect_that!(blank.body["steps"][0]["version"], eq(&json!(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_step_version_conflicts_and_a_missing_one_is_required() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Act
    let stale = edit(&app, &session, &heap, &key, "1", &json!(1), json!({ "state": "open" })).await?;
    let missing = app
        .send(
            Method::PATCH,
            &format!("{}/steps/1", item_path(&heap, &key, &app).await?),
            Some(&session),
            Some(json!({ "state": "open" })),
        )
        .await?;

    // Assert
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(missing.status, eq(StatusCode::PRECONDITION_REQUIRED));
    expect_that!(missing.body["code"], eq(&json!("version_required")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_step_the_item_does_not_have_is_not_found() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    let other = app.new_item(&session, &heap, json!({ "title": "Another" })).await?;

    // Act
    let mut statuses = Vec::new();
    for step in ["9", "0", "-1", "abc"] {
        statuses.push(
            edit(
                &app,
                &session,
                &heap,
                &key,
                step,
                &json!(1),
                json!({ "state": "ticked" }),
            )
            .await?
            .status,
        );
    }
    let other_key = other["key"].as_str().context("no key")?;
    let elsewhere = edit(
        &app,
        &session,
        &heap,
        other_key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Assert
    expect_that!(statuses, each(eq(&StatusCode::NOT_FOUND)));
    expect_that!(elsewhere.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_has_no_steps_and_does_not_offer_adding_one() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let note = app.new_item(&session, &heap, json!({ "title": "A note" })).await?;
    let key = note["key"].as_str().context("no key")?;

    // Act
    let refused = add(&app, &session, &heap, key, json!({ "title": "One" })).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("steps_need_task")));
    expect_that!(note["steps"], eq(&json!([])));
    expect_that!(note["step_progress"], eq(&Value::Null));
    expect_that!(actions(&note), not(contains(eq(&"add_step"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_task_keeps_its_steps_read_only_until_it_is_reopened() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    let added = add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    let closed = app
        .patch(
            &session,
            &item_path(&heap, &key, &app).await?,
            &added.body["version"],
            json!({ "state": "done" }),
        )
        .await?;

    // Act
    let adding = add(&app, &session, &heap, &key, json!({ "title": "Two" })).await?;
    let editing = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;
    let reopened = app
        .patch(
            &session,
            &item_path(&heap, &key, &app).await?,
            &closed.body["version"],
            json!({ "state": "open" }),
        )
        .await?;
    let edited_after = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;

    // Assert
    expect_that!(closed.body["steps"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(actions(&closed.body), not(contains(eq(&"add_step"))));
    expect_that!(adding.status, eq(StatusCode::CONFLICT));
    expect_that!(adding.body["code"], eq(&json!("item_closed")));
    expect_that!(editing.status, eq(StatusCode::CONFLICT));
    expect_that!(editing.body["code"], eq(&json!("item_closed")));
    expect_that!(reopened.body["steps"][0]["allowed_actions"], eq(&json!(["edit"])));
    expect_that!(edited_after.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn after_a_step_the_item_does_not_have_is_refused_naming_the_field() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;

    // Act
    let unknown = add(&app, &session, &heap, &key, json!({ "title": "Two", "after": 9 })).await?;
    let zero = add(&app, &session, &heap, &key, json!({ "title": "Two", "after": 0 })).await?;

    // Assert
    expect_that!(unknown.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(unknown.body["code"], eq(&json!("step_not_in_item")));
    expect_that!(unknown.body["field"], eq(&json!("after")));
    expect_that!(zero.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(zero.body["field"], eq(&json!("after")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_hundred_and_first_step_is_refused() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    for n in 1..=100 {
        let added = add(&app, &session, &heap, &key, json!({ "title": format!("Step {n}") })).await?;
        anyhow::ensure!(added.status == StatusCode::CREATED, "step {n}: {}", added.body);
    }

    // Act
    let one_more = add(&app, &session, &heap, &key, json!({ "title": "One more" })).await?;

    // Assert
    expect_that!(one_more.status, eq(StatusCode::CONFLICT));
    expect_that!(one_more.body["code"], eq(&json!("too_many_steps")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_patch_that_changes_nothing_succeeds_and_writes_no_audit_entry() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    let added = add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    let id = added.body["id"].as_str().context("no id")?;
    let entries_before = app.audit_entries(id).await?.len();

    // Act
    let empty = edit(&app, &session, &heap, &key, "1", &json!(1), json!({})).await?;
    let same = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "title": "One", "state": "open" }),
    )
    .await?;

    // Assert
    expect_that!(empty.status, eq(StatusCode::OK));
    expect_that!(empty.body["steps"][0]["version"], eq(&json!(1)));
    expect_that!(same.status, eq(StatusCode::OK));
    expect_that!(same.body["steps"][0]["version"], eq(&json!(1)));
    expect_that!(app.audit_entries(id).await?.len(), eq(entries_before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_state_the_server_does_not_know_and_a_field_it_does_not_have_are_refused() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;

    // Act
    let state = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "parked" }),
    )
    .await?;
    let field = edit(&app, &session, &heap, &key, "1", &json!(1), json!({ "color": "red" })).await?;
    let blank = add(&app, &session, &heap, &key, json!({ "title": "  " })).await?;
    let extra = add(&app, &session, &heap, &key, json!({ "title": "Two", "color": "red" })).await?;

    // Assert
    expect_that!(state.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(state.body["code"], eq(&json!("value_unknown")));
    expect_that!(state.body["field"], eq(&json!("state")));
    expect_that!(field.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(blank.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(blank.body["field"], eq(&json!("title")));
    expect_that!(extra.status, eq(StatusCode::BAD_REQUEST));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_lists_carry_each_tasks_progress_but_not_its_steps() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;
    let note = app.new_item(&session, &heap, json!({ "title": "A note" })).await?;

    // Act
    let ranked = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let tasks = ranked.body["ranked"]
        .as_array()
        .context("no ranked items")?
        .iter()
        .map(|entry| entry["item"].clone())
        .collect::<Vec<_>>();
    let listed_task = tasks
        .iter()
        .find(|item| item["key"] == json!(key))
        .context("task not listed")?;
    let listed_note = tasks
        .iter()
        .find(|item| item["key"] == note["key"])
        .context("note not listed")?;

    // Assert
    expect_that!(listed_task["steps"], eq(&Value::Null));
    expect_that!(listed_task["step_progress"], eq(&json!({ "ticked": 1, "total": 1 })));
    expect_that!(actions(listed_task), contains(eq(&"add_step")));
    expect_that!(listed_note["steps"], eq(&Value::Null));
    expect_that!(listed_note["step_progress"], eq(&Value::Null));
    expect_that!(actions(listed_note), not(contains(eq(&"add_step"))));

    // Act: close the task and read the closed list.
    let fetched = get(&app, &session, &heap, &key).await?;
    app.patch(
        &session,
        &item_path(&heap, &key, &app).await?,
        &fetched["version"],
        json!({ "state": "done" }),
    )
    .await?;
    let closed = app
        .send(
            Method::GET,
            &format!("{}/items/closed", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(closed.body["items"][0]["steps"], eq(&Value::Null));
    expect_that!(
        closed.body["items"][0]["step_progress"],
        eq(&json!({ "ticked": 1, "total": 1 }))
    );

    // Act: the item itself carries the steps, read-only now.
    let item = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(item["steps"][0]["state"], eq(&json!("ticked")));
    expect_that!(item["steps"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(step_version(&item, 1), eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_refuses_step_writes() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = task().await?;
    add(&app, &session, &heap, &key, json!({ "title": "One" })).await?;
    app.set_heap_archived(&heap, true).await?;

    // Act
    let adding = add(&app, &session, &heap, &key, json!({ "title": "Two" })).await?;
    let editing = edit(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "state": "ticked" }),
    )
    .await?;
    let fetched = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(adding.body["code"], eq(&json!("heap_archived")));
    expect_that!(editing.body["code"], eq(&json!("heap_archived")));
    expect_that!(fetched["steps"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(actions(&fetched), not(contains(eq(&"add_step"))));
    Ok(())
}
