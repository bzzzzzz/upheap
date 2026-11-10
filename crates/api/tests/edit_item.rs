//! The patches the Edit item view sends: every changed field in one request, so one save is one version and one
//! audit entry.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

/// Creates an item from `body` and returns its ID.
async fn create(app: &TestApp, session: &str, heap: &str, body: Value) -> Result<String> {
    let item = app.new_item(session, heap, body).await?;
    Ok(item["id"].as_str().unwrap_or_default().to_owned())
}

/// Sends a `PATCH` for item `item` at `version`.
async fn patch(app: &TestApp, session: &str, item: &str, version: u64, body: Value) -> Result<TestResponse> {
    app.patch(session, &app.item_url(&item).await?, &json!(version), body)
        .await
}

#[gtest]
#[tokio::test]
async fn one_save_changes_every_field_in_one_version_and_one_audit_entry() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = create(&app, &session, &heap, json!({ "title": "Upgrade the database" })).await?;
    let item = create(&app, &session, &heap, json!({ "title": "Migrate" })).await?;
    let tag = app.new_tag_id(&session, &heap, "infra").await?;
    // Audit entries in the same millisecond have no fixed order, so the save happens later.
    app.clock.advance(Duration::from_millis(1));

    // Act
    let saved = patch(
        &app,
        &session,
        &item,
        1,
        json!({
            "title": "Migrate the reports",
            "description": "After the upgrade.",
            "effort": "l",
            "impact": "xl",
            "due": { "date": "2027-02-01", "firm": true },
            "tag_ids": [tag],
            "state": "blocked",
            "blocked_by_item_id": blocker,
        }),
    )
    .await?;
    let fetched = app
        .send(Method::GET, &app.item_url(&item).await?, Some(&session), None)
        .await?;
    let audit = app.audit_entries(&item).await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::OK));
    expect_that!(saved.body["version"], eq(&json!(2)));
    expect_that!(fetched.body, eq(&saved.body));
    expect_that!(fetched.body["title"], eq(&json!("Migrate the reports")));
    expect_that!(fetched.body["description"], eq(&json!("After the upgrade.")));
    expect_that!(fetched.body["effort"], eq(&json!("l")));
    expect_that!(fetched.body["impact"], eq(&json!("xl")));
    expect_that!(fetched.body["due"]["date"], eq(&json!("2027-02-01")));
    expect_that!(fetched.body["due"]["firm"], eq(&json!(true)));
    expect_that!(fetched.body["tags"][0]["id"], eq(&json!(tag)));
    expect_that!(fetched.body["state"], eq(&json!("blocked")));
    expect_that!(fetched.body["blocked_by"]["id"], eq(&json!(blocker)));
    let actions: Vec<&str> = audit.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(actions, elements_are![eq(&"item_created"), eq(&"item_updated")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn null_unsets_the_optional_fields() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = create(
        &app,
        &session,
        &heap,
        json!({
            "title": "Task",
            "description": "Details",
            "effort": "s",
            "impact": "m",
            "due": { "date": "2027-02-01", "firm": false },
        }),
    )
    .await?;

    // Act
    let saved = patch(
        &app,
        &session,
        &item,
        1,
        json!({ "description": null, "effort": null, "impact": null, "due": null, "tag_ids": [] }),
    )
    .await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::OK));
    expect_that!(saved.body["description"], eq(&Value::Null));
    expect_that!(saved.body["effort"], eq(&Value::Null));
    expect_that!(saved.body["impact"], eq(&Value::Null));
    expect_that!(saved.body["due"], eq(&Value::Null));
    expect_that!(saved.body["tags"], eq(&json!([])));
    Ok(())
}

/// Creates an item and closes it as done; it's then at version 2.
async fn closed_item(app: &TestApp, session: &str, heap: &str, title: &str) -> Result<String> {
    let item = create(app, session, heap, json!({ "title": title })).await?;
    let done = patch(app, session, &item, 1, json!({ "state": "done" })).await?;
    anyhow::ensure!(done.status == StatusCode::OK, "close failed: {}", done.body);
    Ok(item)
}

#[gtest]
#[tokio::test]
async fn a_closed_item_refuses_edits_while_it_stays_closed() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = closed_item(&app, &session, &heap, "Task").await?;

    // Act
    let edited = patch(&app, &session, &item, 2, json!({ "title": "Edited" })).await?;

    // Assert
    expect_that!(edited.status, eq(StatusCode::CONFLICT));
    expect_that!(edited.body["code"], eq(&json!("item_closed")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn one_save_reopens_a_closed_item_and_edits_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = closed_item(&app, &session, &heap, "Task").await?;

    // Act
    let reopened = patch(&app, &session, &item, 2, json!({ "state": "open", "title": "Edited" })).await?;

    // Assert
    expect_that!(reopened.status, eq(StatusCode::OK));
    expect_that!(reopened.body["state"], eq(&json!("open")));
    expect_that!(reopened.body["title"], eq(&json!("Edited")));
    expect_that!(reopened.body["version"], eq(&json!(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn one_save_reopens_a_closed_item_as_blocked_with_its_blocker() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = create(&app, &session, &heap, json!({ "title": "Upgrade the database" })).await?;
    let item = closed_item(&app, &session, &heap, "Migrate").await?;

    // Act
    let reopened = patch(
        &app,
        &session,
        &item,
        2,
        json!({ "state": "blocked", "blocked_by_item_id": blocker }),
    )
    .await?;

    // Assert
    expect_that!(reopened.status, eq(StatusCode::OK));
    expect_that!(reopened.body["state"], eq(&json!("blocked")));
    expect_that!(reopened.body["blocked_by"]["id"], eq(&json!(blocker)));
    Ok(())
}

/// The item's `allowed_actions` as strings.
fn actions(item: &Value) -> Vec<&str> {
    item["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn a_new_item_is_a_note_as_written_that_can_become_a_task() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;

    // Act
    let item = app
        .new_item(&session, &heap, json!({ "title": "Rotate the certs" }))
        .await?;

    // Assert
    expect_that!(item["kind"], eq(&json!("note")));
    expect_that!(item["stage"], eq(&json!("as_written")));
    expect_that!(item["stage_choices"], eq(&json!(["as_written", "with_context"])));
    expect_that!(actions(&item), contains(eq(&"make_task")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_becomes_a_task_in_place() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let note = app
        .new_item(&session, &heap, json!({ "title": "Rotate the certs" }))
        .await?;
    let id = note["id"].as_str().unwrap_or_default();

    // Act
    let task = patch(&app, &session, id, 1, json!({ "kind": "task" })).await?;

    // Assert
    expect_that!(task.status, eq(StatusCode::OK));
    expect_that!(task.body["kind"], eq(&json!("task")));
    expect_that!(task.body["key"], eq(&note["key"]));
    expect_that!(task.body["created_at"], eq(&note["created_at"]));
    expect_that!(
        task.body["stage_choices"],
        eq(&json!(["as_written", "with_context", "defined"]))
    );
    expect_that!(actions(&task.body), not(contains(eq(&"make_task"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_never_goes_back_to_a_note() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let id = create(&app, &session, &heap, json!({ "title": "Rotate the certs" })).await?;
    patch(&app, &session, &id, 1, json!({ "kind": "task" })).await?;

    // Act
    let back = patch(&app, &session, &id, 2, json!({ "kind": "note" })).await?;

    // Assert
    expect_that!(back.status, eq(StatusCode::CONFLICT));
    expect_that!(back.body["code"], eq(&json!("kind_change_not_allowed")));
    expect_that!(back.body["field"], eq(&json!("kind")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn only_a_task_can_be_defined() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let note = create(&app, &session, &heap, json!({ "title": "A note" })).await?;
    let other = create(&app, &session, &heap, json!({ "title": "Another" })).await?;

    // Act
    let refused = patch(&app, &session, &note, 1, json!({ "stage": "defined" })).await?;
    let with_context = patch(&app, &session, &note, 1, json!({ "stage": "with_context" })).await?;
    let both = patch(&app, &session, &other, 1, json!({ "kind": "task", "stage": "defined" })).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("stage_needs_task")));
    expect_that!(refused.body["field"], eq(&json!("stage")));
    expect_that!(with_context.status, eq(StatusCode::OK));
    expect_that!(with_context.body["stage"], eq(&json!("with_context")));
    expect_that!(both.status, eq(StatusCode::OK));
    expect_that!(both.body["kind"], eq(&json!("task")));
    expect_that!(both.body["stage"], eq(&json!("defined")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_kind_or_stage_the_server_doesnt_know_is_refused_naming_the_field() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = create(&app, &session, &heap, json!({ "title": "A note" })).await?;

    // Act
    let kind = patch(&app, &session, &item, 1, json!({ "kind": "reminder" })).await?;
    let stage = patch(&app, &session, &item, 1, json!({ "stage": "reviewed" })).await?;

    // Assert
    expect_that!(kind.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(kind.body["code"], eq(&json!("value_unknown")));
    expect_that!(kind.body["field"], eq(&json!("kind")));
    expect_that!(stage.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(stage.body["field"], eq(&json!("stage")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_shows_no_stage_refuses_kind_changes_and_keeps_both_when_reopened() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = create(&app, &session, &heap, json!({ "title": "Task" })).await?;
    patch(&app, &session, &item, 1, json!({ "kind": "task", "stage": "defined" })).await?;
    let closed = patch(&app, &session, &item, 2, json!({ "state": "done" })).await?;

    // Act
    let refused = patch(&app, &session, &item, 3, json!({ "kind": "task" })).await?;
    let reopened = patch(&app, &session, &item, 3, json!({ "state": "open" })).await?;

    // Assert
    expect_that!(closed.body["kind"], eq(&json!("task")));
    expect_that!(closed.body["stage"], eq(&Value::Null));
    expect_that!(closed.body["stage_choices"], eq(&json!([])));
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("item_closed")));
    expect_that!(reopened.body["kind"], eq(&json!("task")));
    expect_that!(reopened.body["stage"], eq(&json!("defined")));
    Ok(())
}
