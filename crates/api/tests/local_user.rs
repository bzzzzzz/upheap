//! The local build's one user: made on start with a team and a heap, and the caller of every request without a
//! session.

mod common;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::json;
use service::{Services, Settings, SystemClock};

#[gtest]
#[tokio::test]
async fn the_local_user_is_signed_in_with_one_team_and_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;

    // Act
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;

    // Assert
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(me.body["user"]["display_name"].as_str(), some(eq("Me")));
    let teams = me.body["teams"].as_array().context("teams")?;
    expect_that!(teams.len(), eq(1));
    expect_that!(teams[0]["name"].as_str(), some(eq("My heaps")));
    expect_that!(teams[0]["role"].as_str(), some(eq("admin")));
    let heaps = teams[0]["heaps"].as_array().context("heaps")?;
    expect_that!(heaps.len(), eq(1));
    expect_that!(heaps[0]["name"].as_str(), some(eq("Personal")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_local_user_is_offered_only_what_one_user_can_do() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;
    let team = me.body["teams"][0]["id"].as_str().context("team")?.to_owned();

    // Act
    let detail = app.send(Method::GET, &app.team_url(&team).await?, None, None).await?;

    // Assert
    expect_that!(me.body["allowed_actions"], eq(&json!([])));
    expect_that!(detail.status, eq(StatusCode::OK));
    expect_that!(detail.body["allowed_actions"], eq(&json!(["create_heap"])));
    expect_that!(detail.body["members"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(
        detail.body["heaps"][0]["allowed_actions"],
        eq(&json!([
            "rename",
            "archive",
            "edit_context",
            "edit_weights",
            "set_agents_prepare",
            "view_history"
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_local_user_works_in_their_heap_without_a_session() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;
    let heap = me.body["teams"][0]["heaps"][0]["id"]
        .as_str()
        .context("heap")?
        .to_owned();

    // Act
    let created = app
        .send(
            Method::POST,
            &format!("{}/items", app.heap_url(&heap).await?),
            None,
            Some(json!({ "need": "expected", "title": "Water the plants", "effort": "s", "impact": "m" })),
        )
        .await?;
    let listed = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            None,
            None,
        )
        .await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(listed.status, eq(StatusCode::OK));
    expect_that!(
        listed.body["ranked"][0]["item"]["title"].as_str(),
        some(eq("Water the plants"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn signing_in_is_not_offered_to_the_local_user() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;
    let register = json!({ "email": "ada@example.test", "username": "ada", "display_name": "Ada", "password": "correct horse battery" });

    // Act
    let registered = app
        .send(Method::POST, "/api/v1/auth/register", None, Some(register))
        .await?;
    let signed_out = app.send(Method::POST, "/api/v1/auth/logout", None, None).await?;

    // Assert
    expect_that!(registered.status, eq(StatusCode::NOT_FOUND));
    expect_that!(signed_out.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn opening_the_local_user_again_finds_the_same_user() -> Result<()> {
    // Arrange
    let db = store::testing::fresh_database().await?;
    let services = Services::new(db, Arc::new(SystemClock), Settings::default());
    let first = services.open_local_user().await?;

    // Act
    let again = services.open_local_user().await?;

    // Assert
    expect_that!(again, eq(first));
    let me = services.load_me(again).await?;
    expect_that!(me.teams.len(), eq(1));
    expect_that!(me.teams[0].heaps.len(), eq(1));
    Ok(())
}
