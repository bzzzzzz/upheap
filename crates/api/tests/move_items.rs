//! Moving several items to one heap in a request: each moves on its own, and what didn't is named with its problem.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

fn id(body: &Value) -> String {
    body["id"].as_str().unwrap_or_default().to_owned()
}

fn ids(items: &Value) -> Vec<String> {
    items.as_array().map(|a| a.iter().map(id).collect()).unwrap_or_default()
}

/// The team's URL, from one of its heaps.
async fn team_of(app: &TestApp, heap: &str) -> Result<String> {
    let url = app.heap_url(heap).await?;
    Ok(url.split("/heaps/").next().context("heap url")?.to_owned())
}

async fn new_heap(app: &TestApp, session: &str, heap: &str, name: &str) -> Result<String> {
    let created = app
        .send(
            Method::POST,
            &format!("{}/heaps", team_of(app, heap).await?),
            Some(session),
            Some(json!({ "name": name })),
        )
        .await?;
    anyhow::ensure!(
        created.status == StatusCode::CREATED,
        "create heap failed: {}",
        created.body
    );
    Ok(id(&created.body))
}

/// Posts the batch for `items` (each `(id, version)`) to the team of `team_heap`.
async fn move_items(
    app: &TestApp,
    session: &str,
    team_heap: &str,
    target: &str,
    items: &[(&str, &Value)],
) -> Result<TestResponse> {
    let items: Vec<Value> = items
        .iter()
        .map(|(id, version)| json!({ "id": id, "version": version }))
        .collect();
    app.send_with(
        Method::POST,
        &format!("{}/items/move", team_of(app, team_heap).await?),
        Some(session),
        Some(json!({ "heap_id": target, "items": items })),
        &[CSRF],
    )
    .await
}

async fn ranked_ids(app: &TestApp, session: &str, heap: &str) -> Result<Vec<String>> {
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(heap).await?),
            Some(session),
            None,
        )
        .await?;
    Ok(list.body["ranked"]
        .as_array()
        .map(|entries| entries.iter().map(|e| id(&e["item"])).collect())
        .unwrap_or_default())
}

async fn moved_entries(app: &TestApp, item: &str) -> Result<usize> {
    Ok(app
        .audit_entries(item)
        .await?
        .iter()
        .filter(|(action, _)| action == "item_moved")
        .count())
}

/// A user with two heaps, `from` and `to`, and three open items in `from`.
struct World {
    app: TestApp,
    session: String,
    from: String,
    to: String,
    items: Vec<Value>,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, from) = app.register("ada@example.test").await?;
    let to = new_heap(&app, &session, &from, "Agent work").await?;
    let mut items = Vec::new();
    for title in ["One", "Two", "Three"] {
        items.push(app.new_item(&session, &from, json!({ "title": title })).await?);
    }
    Ok(World {
        app,
        session,
        from,
        to,
        items,
    })
}

impl World {
    async fn move_all(&self, items: &[&Value]) -> Result<TestResponse> {
        let ids: Vec<String> = items.iter().map(|i| id(i)).collect();
        let pairs: Vec<(&str, &Value)> = ids
            .iter()
            .zip(items)
            .map(|(id, item)| (id.as_str(), &item["version"]))
            .collect();
        move_items(&self.app, &self.session, &self.from, &self.to, &pairs).await
    }
}

#[gtest]
#[tokio::test]
async fn three_items_move_in_one_request() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let moved = w.move_all(&[&w.items[0], &w.items[1], &w.items[2]]).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(
        ids(&moved.body["moved"]),
        eq(&w.items.iter().map(id).collect::<Vec<_>>())
    );
    expect_that!(moved.body["not_moved"], eq(&json!([])));
    for item in moved.body["moved"].as_array().into_iter().flatten() {
        expect_that!(item["heap"]["id"], eq(&json!(w.to)));
        expect_that!(moved_entries(&w.app, &id(item)).await?, eq(1));
    }
    expect_that!(ranked_ids(&w.app, &w.session, &w.from).await?, is_empty());
    expect_that!(ranked_ids(&w.app, &w.session, &w.to).await?.len(), eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_is_named_and_the_others_move() -> Result<()> {
    // Arrange
    let w = world().await?;
    let url = w.app.item_url(id(&w.items[1])).await?;
    let closed = w
        .app
        .patch(&w.session, &url, &w.items[1]["version"], json!({ "state": "done" }))
        .await?;

    // Act
    let moved = w.move_all(&[&w.items[0], &closed.body, &w.items[2]]).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(ids(&moved.body["moved"]), eq(&vec![id(&w.items[0]), id(&w.items[2])]));
    expect_that!(moved.body["not_moved"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(moved.body["not_moved"][0]["id"], eq(&json!(id(&w.items[1]))));
    expect_that!(moved.body["not_moved"][0]["problem"]["code"], eq(&json!("item_closed")));
    expect_that!(moved.body["not_moved"][0]["problem"]["status"], eq(&json!(409)));
    expect_that!(moved_entries(&w.app, &id(&w.items[1])).await?, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_version_is_a_conflict_for_that_item_only() -> Result<()> {
    // Arrange
    let w = world().await?;
    let url = w.app.item_url(id(&w.items[0])).await?;
    w.app
        .patch(&w.session, &url, &w.items[0]["version"], json!({ "title": "Changed" }))
        .await?;

    // Act: the first item's version is the old one.
    let moved = w.move_all(&[&w.items[0], &w.items[1], &w.items[2]]).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(ids(&moved.body["moved"]), eq(&vec![id(&w.items[1]), id(&w.items[2])]));
    expect_that!(moved.body["not_moved"][0]["id"], eq(&json!(id(&w.items[0]))));
    expect_that!(
        moved.body["not_moved"][0]["problem"]["code"],
        eq(&json!("version_conflict"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_from_two_heaps_move_to_a_third() -> Result<()> {
    // Arrange
    let w = world().await?;
    let other = new_heap(&w.app, &w.session, &w.from, "Other").await?;
    let third = new_heap(&w.app, &w.session, &w.from, "Third").await?;
    let elsewhere = w
        .app
        .new_item(&w.session, &other, json!({ "title": "Elsewhere" }))
        .await?;
    let pairs = [
        (id(&w.items[0]), w.items[0]["version"].clone()),
        (id(&elsewhere), elsewhere["version"].clone()),
    ];
    let pairs: Vec<(&str, &Value)> = pairs.iter().map(|(i, v)| (i.as_str(), v)).collect();

    // Act
    let moved = move_items(&w.app, &w.session, &w.from, &third, &pairs).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["moved"].as_array().map(Vec::len), some(eq(2)));
    expect_that!(moved.body["not_moved"], eq(&json!([])));
    expect_that!(ranked_ids(&w.app, &w.session, &third).await?.len(), eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_target_in_another_team_moves_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (_, grace_heap) = w.app.register("grace@example.test").await?;

    // Act
    let moved = move_items(
        &w.app,
        &w.session,
        &w.from,
        &grace_heap,
        &w.items
            .iter()
            .map(|i| (i["id"].as_str().unwrap_or_default(), &i["version"]))
            .collect::<Vec<_>>(),
    )
    .await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["moved"], eq(&json!([])));
    let not_moved = moved.body["not_moved"].as_array().cloned().unwrap_or_default();
    expect_that!(not_moved.len(), eq(3));
    for entry in &not_moved {
        expect_that!(entry["problem"]["code"], eq(&json!("heap_not_in_team")));
    }
    expect_that!(ranked_ids(&w.app, &w.session, &w.from).await?.len(), eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_empty_list_succeeds_with_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let moved = move_items(&w.app, &w.session, &w.from, &w.to, &[]).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body, eq(&json!({ "moved": [], "not_moved": [] })));
    Ok(())
}
