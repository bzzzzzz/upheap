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
    app.patch(session, &format!("/api/v1/items/{item}"), &json!(version), body)
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
        .send(Method::GET, &format!("/api/v1/items/{item}"), Some(&session), None)
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
