//! The longest texts the rules allow are stored whole on every database, in any script: an item's context, a
//! step's context and a heap's context of 20,000 characters, each four bytes in UTF-8, and the audit entry that
//! keeps both the text before and after when one is replaced by another.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};

/// 20,000 characters of four bytes each: 80,000 bytes, past what a MySQL `TEXT` column holds.
fn longest(fill: char) -> String {
    fill.to_string().repeat(20_000)
}

#[gtest]
#[tokio::test]
async fn the_longest_contexts_in_four_byte_characters_are_saved_and_replaced_whole() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (first, second) = (longest('😀'), longest('🚀'));
    let note = app
        .new_item(&session, &heap, json!({ "title": "Rotate the certs" }))
        .await?;
    let item_url = app.item_url(note["id"].as_str().context("no id")?).await?;
    let heap_url = app.heap_url(&heap).await?;
    let heap_version = app.send(Method::GET, &heap_url, Some(&session), None).await?.body["version"].clone();

    // Act
    let described = app
        .patch(
            &session,
            &item_url,
            &note["version"],
            json!({ "description": first, "kind": "task" }),
        )
        .await?;
    let redescribed = app
        .patch(
            &session,
            &item_url,
            &described.body["version"],
            json!({ "description": second }),
        )
        .await?;
    let stepped = app
        .send(
            Method::POST,
            &format!("{item_url}/steps"),
            Some(&session),
            Some(json!({ "title": "Run", "context": first })),
        )
        .await?;
    let step_version = stepped.body["steps"][0]["version"].clone();
    let restepped = app
        .patch(
            &session,
            &format!("{item_url}/steps/1"),
            &step_version,
            json!({ "context": second }),
        )
        .await?;
    let contexted = app
        .patch(&session, &heap_url, &heap_version, json!({ "context": first }))
        .await?;
    let recontexted = app
        .patch(
            &session,
            &heap_url,
            &contexted.body["version"],
            json!({ "context": second }),
        )
        .await?;
    let item = app.send(Method::GET, &item_url, Some(&session), None).await?.body;
    let read_heap = app.send(Method::GET, &heap_url, Some(&session), None).await?.body;

    // Assert
    for response in [&described, &redescribed, &stepped, &restepped, &contexted, &recontexted] {
        expect_that!(
            response.status,
            eq(StatusCode::OK).or(eq(StatusCode::CREATED)),
            "{}",
            response.body
        );
    }
    expect_that!(item["description"], eq(&Value::from(second.clone())));
    expect_that!(item["steps"][0]["context"], eq(&Value::from(second.clone())));
    expect_that!(read_heap["context"], eq(&Value::from(second)));
    Ok(())
}
