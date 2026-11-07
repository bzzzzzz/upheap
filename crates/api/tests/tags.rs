//! Tags: creating and renaming them, and putting them on items.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};

fn tag_names(item: &Value) -> Vec<&str> {
    item["tags"]
        .as_array()
        .map(|tags| tags.iter().filter_map(|t| t["name"].as_str()).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn items_carry_their_tags_by_name() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let backend = app.new_tag_id(&session, &heap, "Backend").await?;

    // Act
    let created = app
        .create_item(
            &session,
            &heap,
            json!({ "title": "Rotate keys", "tag_ids": [ops, backend, ops] }),
        )
        .await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let tags = app
        .send(
            Method::GET,
            &format!("{}/tags", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(tag_names(&created.body), elements_are![eq(&"Backend"), eq(&"ops")]);
    expect_that!(
        tag_names(&list.body["ranked"][0]["item"]),
        elements_are![eq(&"Backend"), eq(&"ops")]
    );
    expect_that!(tags.body["tags"].as_array().map(Vec::len), some(eq(2)));
    expect_that!(tags.body["allowed_actions"], eq(&json!(["create"])));
    expect_that!(tags.body["tags"][0]["allowed_actions"], eq(&json!(["edit"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_names_are_unique_per_heap_ignoring_case() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    app.new_tag_id(&session, &heap, "Infra").await?;
    let ops = app.create_tag(&session, &heap, "ops").await?;
    let ops_path = format!(
        "{}/tags/{}",
        app.heap_url(&heap).await?,
        ops.body["id"].as_str().unwrap_or_default()
    );

    // Act
    let same_heap = app.create_tag(&session, &heap, "  infra ").await?;
    let other_heap = app.create_tag(&other_session, &other_heap, "Infra").await?;
    let rename_onto = app
        .patch(&session, &ops_path, &ops.body["version"], json!({ "name": "INFRA" }))
        .await?;
    let recase_self = app
        .patch(&session, &ops_path, &ops.body["version"], json!({ "name": "Ops" }))
        .await?;

    // Assert
    expect_that!(same_heap.status, eq(StatusCode::CONFLICT));
    expect_that!(same_heap.body["code"], eq(&json!("tag_name_taken")));
    expect_that!(other_heap.status, eq(StatusCode::CREATED));
    expect_that!(rename_onto.body["code"], eq(&json!("tag_name_taken")));
    expect_that!(recase_self.status, eq(StatusCode::OK));
    expect_that!(recase_self.body["name"], eq(&json!("Ops")));
    expect_that!(recase_self.body["version"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn renaming_a_tag_renames_it_on_every_item() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let tag = app.create_tag(&session, &heap, "infra").await?;
    let tag_id = tag.body["id"].as_str().unwrap_or_default();
    let item = app
        .create_item(&session, &heap, json!({ "title": "Task", "tag_ids": [tag_id] }))
        .await?;

    // Act
    let renamed = app
        .patch(
            &session,
            &format!("{}/tags/{tag_id}", app.heap_url(&heap).await?),
            &tag.body["version"],
            json!({ "name": "platform", "description": "Shared services" }),
        )
        .await?;
    let reloaded = app
        .send(
            Method::GET,
            &app.item_url(item.body["id"].as_str().unwrap_or_default()).await?,
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(renamed.body["description"], eq(&json!("Shared services")));
    expect_that!(tag_names(&reloaded.body), elements_are![eq(&"platform")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tags_can_be_removed_from_an_item_and_added_back() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let docs = app.new_tag_id(&session, &heap, "docs").await?;
    let item = app
        .create_item(&session, &heap, json!({ "title": "Task", "tag_ids": [ops] }))
        .await?;
    let path = app.item_url(item.body["id"].as_str().unwrap_or_default()).await?;

    // Act
    let swapped = app
        .patch(&session, &path, &item.body["version"], json!({ "tag_ids": [docs] }))
        .await?;
    let cleared = app
        .patch(&session, &path, &swapped.body["version"], json!({ "tag_ids": [] }))
        .await?;
    let restored = app
        .patch(
            &session,
            &path,
            &cleared.body["version"],
            json!({ "tag_ids": [ops, docs] }),
        )
        .await?;
    let unchanged = app
        .patch(
            &session,
            &path,
            &restored.body["version"],
            json!({ "tag_ids": [docs, ops] }),
        )
        .await?;

    // Assert
    expect_that!(tag_names(&swapped.body), elements_are![eq(&"docs")]);
    expect_that!(tag_names(&cleared.body), is_empty());
    expect_that!(tag_names(&restored.body), elements_are![eq(&"docs"), eq(&"ops")]);
    expect_that!(unchanged.body["version"], eq(&restored.body["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tags_from_another_heap_are_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    let foreign = app.new_tag_id(&other_session, &other_heap, "theirs").await?;
    let item = app.create_item(&session, &heap, json!({ "title": "Task" })).await?;
    let path = app.item_url(item.body["id"].as_str().unwrap_or_default()).await?;

    // Act
    let on_create = app
        .create_item(&session, &heap, json!({ "title": "x", "tag_ids": [foreign] }))
        .await?;
    let on_edit = app
        .patch(&session, &path, &item.body["version"], json!({ "tag_ids": [foreign] }))
        .await?;
    let unknown = app
        .create_item(
            &session,
            &heap,
            json!({ "title": "x", "tag_ids": ["00000000-0000-7000-8000-000000000000"] }),
        )
        .await?;

    // Assert
    for response in [&on_create, &on_edit, &unknown] {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(response.body["code"], eq(&json!("tag_not_in_heap")));
        expect_that!(response.body["field"], eq(&json!("tag_ids")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn closed_items_keep_their_tags_until_reopened() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let item = app
        .create_item(&session, &heap, json!({ "title": "Task", "tag_ids": [ops] }))
        .await?;
    let path = app.item_url(item.body["id"].as_str().unwrap_or_default()).await?;
    let done = app
        .patch(&session, &path, &item.body["version"], json!({ "state": "done" }))
        .await?;

    // Act
    let retag = app
        .patch(&session, &path, &done.body["version"], json!({ "tag_ids": [] }))
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
    expect_that!(retag.status, eq(StatusCode::CONFLICT));
    expect_that!(retag.body["code"], eq(&json!("item_closed")));
    expect_that!(tag_names(&closed.body["items"][0]), elements_are![eq(&"ops")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn taking_a_tag_off_an_item_is_audited() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let item = app
        .create_item(&session, &heap, json!({ "title": "Task", "tag_ids": [ops] }))
        .await?;
    let item_id = item.body["id"].as_str().unwrap_or_default().to_owned();

    // Act
    app.patch(
        &session,
        &app.item_url(&item_id).await?,
        &item.body["version"],
        json!({ "tag_ids": [] }),
    )
    .await?;
    let audit = app.audit_entries(&item_id).await?;

    // Assert
    let updated = audit
        .iter()
        .find(|(action, _)| action == "item_updated")
        .context("no item_updated entry")?;
    expect_that!(updated.1, eq(&json!({ "tags": { "before": [ops], "after": [] } })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn null_tag_ids_are_refused_rather_than_ignored() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let item = app
        .create_item(&session, &heap, json!({ "title": "Task", "tag_ids": [ops] }))
        .await?;
    let path = app.item_url(item.body["id"].as_str().unwrap_or_default()).await?;

    // Act
    let null = app
        .patch(&session, &path, &item.body["version"], json!({ "tag_ids": null }))
        .await?;

    // Assert
    expect_that!(null.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(null.body["code"], eq(&json!("invalid_body")));
    Ok(())
}
