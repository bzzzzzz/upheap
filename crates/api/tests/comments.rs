//! Comments on an item: adding, numbering, who may, and the rules around them.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

/// Registers a user and makes an open note in their heap. Returns the app, the session, the heap and the item's key.
async fn note() -> Result<(TestApp, String, String, String)> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = app
        .new_item(&session, &heap, json!({ "title": "Rotate the certs" }))
        .await?;
    let key = item["key"].as_str().context("no key")?.to_owned();
    Ok((app, session, heap, key))
}

async fn item_path(app: &TestApp, heap: &str, key: &str) -> Result<String> {
    Ok(format!("{}/items/{key}", app.heap_url(heap).await?))
}

async fn comment(app: &TestApp, session: &str, heap: &str, key: &str, body: Value) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/comments", item_path(app, heap, key).await?),
        Some(session),
        Some(body),
    )
    .await
}

async fn get(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<Value> {
    let fetched = app
        .send(Method::GET, &item_path(app, heap, key).await?, Some(session), None)
        .await?;
    Ok(fetched.body)
}

fn actions(value: &Value) -> Vec<&str> {
    value["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn comments_are_numbered_in_order_and_leave_the_items_version_alone() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let before = get(&app, &session, &heap, &key).await?;

    // Act
    let first = comment(
        &app,
        &session,
        &heap,
        &key,
        json!({ "body": "  Use the internal CA\nfor staging.  " }),
    )
    .await?;
    app.clock.advance(domain::Duration::from_days(2));
    let second = comment(&app, &session, &heap, &key, json!({ "body": "Done." })).await?;
    let after = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::CREATED));
    expect_that!(first.body["comments"].as_array().map(Vec::len), some(eq(1)));
    let one = &first.body["comments"][0];
    expect_that!(one["number"], eq(&json!(1)));
    expect_that!(one["body"], eq(&json!("Use the internal CA\nfor staging.")));
    expect_that!(one["created_by"]["display_name"], eq(&json!("Tester")));
    expect_that!(one["created_at"].is_i64(), eq(true));
    expect_that!(one["created_label"].is_string(), eq(true));
    expect_that!(second.status, eq(StatusCode::CREATED));
    expect_that!(second.body["comments"][1]["number"], eq(&json!(2)));
    expect_that!(second.body["comments"][1]["body"], eq(&json!("Done.")));
    expect_that!(second.body["version"], eq(&before["version"]));
    expect_that!(after["version"], eq(&before["version"]));
    expect_that!(after["comments"], eq(&second.body["comments"]));
    expect_that!(before["comments"], eq(&json!([])));
    // The label is worded when the item is read: the first comment is two days old by then.
    expect_that!(
        after["comments"][0]["created_label"],
        not(eq(&after["comments"][1]["created_label"]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_takes_a_comment_and_offers_it() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let item = get(&app, &session, &heap, &key).await?;
    let closed = app
        .patch(
            &session,
            &item_path(&app, &heap, &key).await?,
            &item["version"],
            json!({ "state": "done" }),
        )
        .await?;
    anyhow::ensure!(closed.status == StatusCode::OK, "close failed: {}", closed.body);

    // Act
    let added = comment(&app, &session, &heap, &key, json!({ "body": "Rotated on Friday." })).await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::CREATED));
    expect_that!(added.body["state"], eq(&json!("done")));
    expect_that!(added.body["comments"][0]["body"], eq(&json!("Rotated on Friday.")));
    expect_that!(actions(&added.body), elements_are![eq(&"reopen"), eq(&"comment")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_open_item_offers_the_comment_action() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;

    // Act
    let item = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(actions(&item), contains(eq(&"comment")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_comment_in_an_archived_heap_is_refused() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    app.set_heap_archived(&heap, true).await?;

    // Act
    let refused = comment(&app, &session, &heap, &key, json!({ "body": "Too late" })).await?;
    let item = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("heap_archived")));
    expect_that!(actions(&item), not(contains(eq(&"comment"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blank_or_too_long_body_is_refused_on_the_body_field() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;

    // Act
    let blank = comment(&app, &session, &heap, &key, json!({ "body": " \n\t " })).await?;
    let too_long = comment(&app, &session, &heap, &key, json!({ "body": "x".repeat(20_001) })).await?;
    let longest = comment(&app, &session, &heap, &key, json!({ "body": "é".repeat(20_000) })).await?;

    // Assert
    expect_that!(blank.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(blank.body["field"], eq(&json!("body")));
    expect_that!(too_long.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(too_long.body["field"], eq(&json!("body")));
    expect_that!(longest.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_field_or_a_missing_body_is_an_invalid_body() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;

    // Act
    let unknown = comment(&app, &session, &heap, &key, json!({ "body": "Hi", "text": "extra" })).await?;
    let missing = comment(&app, &session, &heap, &key, json!({})).await?;

    // Assert
    expect_that!(unknown.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(unknown.body["code"], eq(&json!("invalid_body")));
    expect_that!(missing.status, eq(StatusCode::BAD_REQUEST));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_has_at_most_two_hundred_comments() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    for n in 1..=200 {
        let added = comment(&app, &session, &heap, &key, json!({ "body": format!("Comment {n}") })).await?;
        anyhow::ensure!(added.status == StatusCode::CREATED, "comment {n}: {}", added.body);
    }

    // Act
    let one_more = comment(&app, &session, &heap, &key, json!({ "body": "One more" })).await?;
    let item = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(one_more.status, eq(StatusCode::CONFLICT));
    expect_that!(one_more.body["code"], eq(&json!("too_many_comments")));
    expect_that!(item["comments"].as_array().map(Vec::len), some(eq(200)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn list_rows_carry_no_comments() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    comment(&app, &session, &heap, &key, json!({ "body": "Hello" })).await?;

    // Act
    let listed = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(listed.status, eq(StatusCode::OK));
    let rows = listed.body["ranked"].as_array().context("no rows")?;
    expect_that!(rows.is_empty(), eq(false));
    for row in rows {
        expect_that!(row["item"]["comments"], eq(&Value::Null));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_items_history_says_who_commented_and_what() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let item = get(&app, &session, &heap, &key).await?;
    let id = item["id"].as_str().context("no id")?;
    app.clock.advance(domain::Duration::from_millis(1));
    comment(&app, &session, &heap, &key, json!({ "body": "Use the internal CA." })).await?;

    // Act
    let history = app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={id}", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let entries = app.audit_entries(id).await?;

    // Assert
    let entry = &history.body["entries"][0];
    expect_that!(entry["action"], eq(&json!("comment_added")));
    expect_that!(entry["phrase"], eq(&json!("commented on")));
    expect_that!(
        entry["changes"],
        eq(&json!([
            { "field": "comment", "label": "Comment", "before": null, "after": "c1" },
            { "field": "comment_text", "label": "Comment text", "before": null, "after": "Use the internal CA." },
        ]))
    );
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(actions, elements_are![eq(&"item_created"), eq(&"comment_added")]);
    expect_that!(
        entries[1].1,
        eq(&json!({ "comment": 1, "comment_text": "Use the internal CA." }))
    );
    Ok(())
}
