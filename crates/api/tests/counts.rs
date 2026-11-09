//! The counts the Teams and Team pages show: each heap's open items and each team's members.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp};
use googletest::prelude::*;
use serde_json::{Value, json};

/// The heap's `open_count` as `/me`, the team and the heap itself report it, in that order.
async fn open_counts(app: &TestApp, session: Option<&str>, team: &str, heap: &str) -> Result<[Value; 3]> {
    let me = app.send(Method::GET, "/api/v1/me", session, None).await?;
    let in_me = me.body["teams"]
        .as_array()
        .and_then(|teams| teams.iter().find(|t| t["id"] == team))
        .and_then(|t| t["heaps"].as_array())
        .and_then(|heaps| heaps.iter().find(|h| h["id"] == heap))
        .map(|h| h["open_count"].clone())
        .unwrap_or(Value::Null);
    let detail = app.send(Method::GET, &app.team_url(team).await?, session, None).await?;
    let in_team = detail.body["heaps"]
        .as_array()
        .and_then(|heaps| heaps.iter().find(|h| h["id"] == heap))
        .map(|h| h["open_count"].clone())
        .unwrap_or(Value::Null);
    let own = app.send(Method::GET, &app.heap_url(heap).await?, session, None).await?;
    Ok([in_me, in_team, own.body["open_count"].clone()])
}

/// The team's `member_count` in `/me`.
async fn member_count(app: &TestApp, session: &str, team: &str) -> Result<Value> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    Ok(me.body["teams"]
        .as_array()
        .and_then(|teams| teams.iter().find(|t| t["id"] == team))
        .map(|t| t["member_count"].clone())
        .unwrap_or(Value::Null))
}

/// Patches the item `item`, as returned by the API, and returns it as saved.
async fn patch_item(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<Value> {
    let url = app.item_url(item["id"].as_str().context("item has no id")?).await?;
    let saved = app.patch(session, &url, &item["version"], body).await?;
    anyhow::ensure!(saved.status == StatusCode::OK, "patch failed: {}", saved.body);
    Ok(saved.body)
}

#[gtest]
#[tokio::test]
async fn a_heap_counts_what_its_list_shows_by_default() -> Result<()> {
    // Arrange: a team heap with four items.
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (team, heap) = app.shared_heap(&ada).await?;
    let new = open_counts(&app, Some(&ada), &team, &heap).await?;
    let mut items = Vec::new();
    for title in ["Ship", "Write", "Test", "Wait"] {
        items.push(app.new_item(&ada, &heap, json!({ "title": title })).await?);
    }
    let added = open_counts(&app, Some(&ada), &team, &heap).await?;

    // Act: one starts, one is blocked by another, and one closes.
    patch_item(&app, &ada, &items[0], json!({ "state": "in_progress" })).await?;
    let blocker = items[1]["id"].clone();
    patch_item(
        &app,
        &ada,
        &items[3],
        json!({ "state": "blocked", "blocked_by_item_id": blocker }),
    )
    .await?;
    let done = patch_item(&app, &ada, &items[2], json!({ "state": "done" })).await?;
    let changed = open_counts(&app, Some(&ada), &team, &heap).await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&ada),
            None,
        )
        .await?;
    let reopened = app
        .send_with(
            Method::POST,
            &format!("{}/reopen", app.item_url(done["id"].as_str().context("no id")?).await?),
            Some(&ada),
            None,
            &[CSRF, ("if-match", &done["version"].to_string())],
        )
        .await?;
    let after_reopen = open_counts(&app, Some(&ada), &team, &heap).await?;

    // Assert: in progress counts, blocked and closed don't, and every response agrees with the list.
    expect_that!(new, eq(&[json!(0), json!(0), json!(0)]));
    expect_that!(added, eq(&[json!(4), json!(4), json!(4)]));
    expect_that!(changed, eq(&[json!(2), json!(2), json!(2)]));
    expect_that!(list.body["unfiltered_count"], eq(&json!(2)));
    expect_that!(reopened.status, eq(StatusCode::OK));
    expect_that!(after_reopen, eq(&[json!(3), json!(3), json!(3)]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_keeps_its_count_and_heaps_count_apart() -> Result<()> {
    // Arrange: two heaps in a team, with two items and one.
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (team, heap) = app.shared_heap(&ada).await?;
    let other = app
        .send(
            Method::POST,
            &format!("{}/heaps", app.team_url(&team).await?),
            Some(&ada),
            Some(json!({ "name": "Other" })),
        )
        .await?;
    let other = other.body["id"].as_str().context("no id")?.to_owned();
    app.new_item(&ada, &heap, json!({ "title": "One" })).await?;
    app.new_item(&ada, &heap, json!({ "title": "Two" })).await?;
    app.new_item(&ada, &other, json!({ "title": "Three" })).await?;

    // Act
    let archived = app
        .send(
            Method::POST,
            &format!("{}/archive", app.heap_url(&heap).await?),
            Some(&ada),
            None,
        )
        .await?;
    let counts = open_counts(&app, Some(&ada), &team, &heap).await?;
    let other_counts = open_counts(&app, Some(&ada), &team, &other).await?;

    // Assert
    expect_that!(archived.body["open_count"], eq(&json!(2)));
    expect_that!(counts, eq(&[json!(2), json!(2), json!(2)]));
    expect_that!(other_counts, eq(&[json!(1), json!(1), json!(1)]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_counts_its_members_as_they_join_and_go() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let (team, _) = app.shared_heap(&ada).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&grace), None).await?;
    let grace_id = me.body["user"]["id"].as_str().context("no id")?.to_owned();
    let personal = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    let alone = member_count(&app, &ada, &team).await?;

    // Act
    let added = app
        .send(
            Method::POST,
            &format!("{}/members", app.team_url(&team).await?),
            Some(&ada),
            Some(json!({ "user_id": grace_id })),
        )
        .await?;
    let together = member_count(&app, &ada, &team).await?;
    let seen_by_grace = member_count(&app, &grace, &team).await?;
    let removed = app
        .send(
            Method::DELETE,
            &app.member_url(&team, &grace_id).await?,
            Some(&ada),
            None,
        )
        .await?;
    let after = member_count(&app, &ada, &team).await?;

    // Assert
    expect_that!(alone, eq(&json!(1)));
    expect_that!(added.status, eq(StatusCode::CREATED));
    expect_that!(together, eq(&json!(2)));
    expect_that!(seen_by_grace, eq(&json!(2)));
    expect_that!(removed.status, eq(StatusCode::NO_CONTENT));
    expect_that!(after, eq(&json!(1)));
    expect_that!(member_count(&app, &grace, &personal).await?, eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_local_build_reports_the_same_counts() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;
    let team = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    let heap = me.body["teams"][0]["heaps"][0]["id"]
        .as_str()
        .context("no heap")?
        .to_owned();

    // Act
    app.send(
        Method::POST,
        &format!("{}/items", app.heap_url(&heap).await?),
        None,
        Some(json!({ "title": "Mine", "need": "expected" })),
    )
    .await?;
    let counts = open_counts(&app, None, &team, &heap).await?;
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;

    // Assert
    expect_that!(counts, eq(&[json!(1), json!(1), json!(1)]));
    expect_that!(me.body["teams"][0]["member_count"], eq(&json!(1)));
    Ok(())
}
