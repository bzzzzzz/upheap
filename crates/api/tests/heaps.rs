//! Heaps: creating, renaming, archiving and unarchiving them.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::json;

/// Registers Ada and returns her session and personal team ID.
async fn ada(app: &TestApp) -> Result<(String, String)> {
    let (session, _) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let team = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    Ok((session, team))
}

async fn create_heap(app: &TestApp, session: &str, team: &str, name: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/teams/{team}/heaps"),
        Some(session),
        Some(json!({ "name": name })),
    )
    .await
}

async fn post(app: &TestApp, session: &str, path: &str) -> Result<TestResponse> {
    app.send(Method::POST, path, Some(session), None).await
}

#[gtest]
#[tokio::test]
async fn a_new_heap_is_listed_for_every_member_of_its_team() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, team) = ada(&app).await?;

    // Act
    let created = create_heap(&app, &session, &team, "Ops").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let heap = created.body["id"].as_str().context("no id")?;
    let list = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/items"),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(created.body["name"], eq(&json!("Ops")));
    expect_that!(created.body["archived_at"], eq(&json!(null)));
    expect_that!(created.body["allowed_actions"], eq(&json!(["rename", "archive"])));
    let names: Vec<&str> = me.body["teams"][0]["heaps"]
        .as_array()
        .map(|heaps| heaps.iter().filter_map(|h| h["name"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(names, elements_are![eq(&"Ops"), eq(&"Tester")]);
    expect_that!(list.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_is_read_only_until_unarchived() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, team) = ada(&app).await?;
    let created = create_heap(&app, &session, &team, "Ops").await?;
    let heap = created.body["id"].as_str().context("no id")?.to_owned();
    app.new_item(&session, &heap, json!({ "title": "Kept" })).await?;

    // Act
    let archived = post(&app, &session, &format!("/api/v1/heaps/{heap}/archive")).await?;
    let again = post(&app, &session, &format!("/api/v1/heaps/{heap}/archive")).await?;
    let write = app.create_item(&session, &heap, json!({ "title": "Refused" })).await?;
    let rename = app
        .patch(
            &session,
            &format!("/api/v1/heaps/{heap}"),
            &archived.body["version"],
            json!({ "name": "X" }),
        )
        .await?;
    let list = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/items"),
            Some(&session),
            None,
        )
        .await?;
    let unarchived = post(&app, &session, &format!("/api/v1/heaps/{heap}/unarchive")).await?;
    let write_after = app.create_item(&session, &heap, json!({ "title": "Accepted" })).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(archived.body["archived_at"], eq(&json!(common::START)));
    expect_that!(archived.body["allowed_actions"], eq(&json!(["unarchive"])));
    expect_that!(again.body["version"], eq(&archived.body["version"]));
    expect_that!(write.body["code"], eq(&json!("heap_archived")));
    expect_that!(rename.body["code"], eq(&json!("heap_archived")));
    expect_that!(list.status, eq(StatusCode::OK));
    expect_that!(list.body["ranked"][0]["item"]["allowed_actions"], eq(&json!([])));
    expect_that!(unarchived.body["archived_at"], eq(&json!(null)));
    expect_that!(write_after.status, eq(StatusCode::CREATED));
    let entries = app.audit_entries(&heap).await?;
    let actions: Vec<&str> = entries.iter().map(|(a, _)| a.as_str()).collect();
    expect_that!(
        actions,
        elements_are![eq(&"heap_created"), eq(&"heap_archived"), eq(&"heap_unarchived")]
    );
    expect_that!(
        entries.get(1).map(|(_, diff)| diff),
        some(eq(
            &json!({ "archived_at": { "before": null, "after": common::START } })
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn renaming_a_heap_needs_its_current_version() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, team) = ada(&app).await?;
    let created = create_heap(&app, &session, &team, "Ops").await?;
    let heap = created.body["id"].as_str().context("no id")?.to_owned();
    let path = format!("/api/v1/heaps/{heap}");

    // Act
    let renamed = app
        .patch(&session, &path, &created.body["version"], json!({ "name": "Infra" }))
        .await?;
    let stale = app
        .patch(&session, &path, &created.body["version"], json!({ "name": "Ops 2" }))
        .await?;
    let fetched = app.send(Method::GET, &path, Some(&session), None).await?;

    // Assert
    expect_that!(renamed.body["name"], eq(&json!("Infra")));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(fetched.body["name"], eq(&json!("Infra")));
    expect_that!(fetched.body["version"], eq(&renamed.body["version"]));
    Ok(())
}
