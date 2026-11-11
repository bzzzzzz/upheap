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
    // Setting that agents prepare new notes is only in a build that has agents.
    let heap_actions = if cfg!(feature = "agents") {
        json!([
            "rename",
            "archive",
            "edit_context",
            "edit_weights",
            "set_agents_prepare",
            "view_history"
        ])
    } else {
        json!(["rename", "archive", "edit_context", "edit_weights", "view_history"])
    };
    expect_that!(detail.body["heaps"][0]["allowed_actions"], eq(&heap_actions));
    Ok(())
}

#[cfg(not(feature = "agents"))]
#[gtest]
#[tokio::test]
async fn a_build_without_agents_offers_no_turns() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;
    let heap = me.body["teams"][0]["heaps"][0]["id"]
        .as_str()
        .context("heap")?
        .to_owned();
    let heap_url = app.heap_url(&heap).await?;
    let created = app
        .send(
            Method::POST,
            &format!("{heap_url}/items"),
            None,
            Some(json!({ "need": "expected", "title": "Water the plants" })),
        )
        .await?;
    let key = created.body["key"].as_str().context("key")?.to_owned();
    let heap_body = app.send(Method::GET, &heap_url, None, None).await?;

    // Act
    let turn_url = format!("{heap_url}/items/{key}/turn");
    let sent = app
        .send(Method::POST, &turn_url, None, Some(json!({ "reach": "one" })))
        .await?;
    let taken = app.send(Method::DELETE, &turn_url, None, None).await?;
    let retried = app.send(Method::POST, &format!("{turn_url}/retry"), None, None).await?;
    let next = app
        .send(Method::GET, "/api/v1/work/next?heaps=me/personal", None, None)
        .await?;
    let claimed = app
        .send(
            Method::POST,
            "/api/v1/work/runs",
            None,
            Some(json!({ "heaps": ["me/personal"] })),
        )
        .await?;
    // A build without runs ignores the header, even a malformed one: the write is just a person's.
    let item_url = format!("{heap_url}/items/{key}");
    let with_header = app
        .send_with(
            Method::POST,
            &format!("{item_url}/steps"),
            None,
            Some(json!({ "title": "A step" })),
            &[common::CSRF, ("upheap-run", "not-a-run")],
        )
        .await?;
    let set = app
        .send_with(
            Method::PATCH,
            &heap_url,
            None,
            Some(json!({ "agents_prepare": true })),
            &[common::CSRF, ("if-match", &heap_body.body["version"].to_string())],
        )
        .await?;

    // Assert
    let actions = created.body["allowed_actions"].as_array().context("actions")?;
    expect_that!(actions.contains(&json!("send_to_agent")), eq(false));
    expect_that!(actions.contains(&json!("take_back")), eq(false));
    expect_that!(created.body["next_goal"], eq(&json!(null)));
    expect_that!(sent.status, eq(StatusCode::NOT_FOUND));
    expect_that!(taken.status, eq(StatusCode::NOT_FOUND));
    expect_that!(retried.status, eq(StatusCode::NOT_FOUND));
    expect_that!(next.status, eq(StatusCode::NOT_FOUND));
    expect_that!(claimed.status, eq(StatusCode::NOT_FOUND));
    // The note can't take steps, but the answer is the person's, not a run's or a header's.
    expect_that!(with_header.status, not(eq(StatusCode::BAD_REQUEST)));
    expect_that!(with_header.status, not(eq(StatusCode::FORBIDDEN)));
    expect_that!(set.status, eq(StatusCode::FORBIDDEN));
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
    let password = json!({ "current_password": "correct horse battery", "password": "another horse battery" });
    let changed = app
        .send(Method::POST, "/api/v1/me/password", None, Some(password))
        .await?;
    let token = "a".repeat(43);
    let reset_opened = app
        .send(Method::GET, &format!("/api/v1/password-resets/{token}"), None, None)
        .await?;
    let reset_used = app
        .send(
            Method::POST,
            &format!("/api/v1/password-resets/{token}"),
            None,
            Some(json!({ "password": "another horse battery" })),
        )
        .await?;

    // Assert
    expect_that!(registered.status, eq(StatusCode::NOT_FOUND));
    expect_that!(signed_out.status, eq(StatusCode::NOT_FOUND));
    expect_that!(changed.status, eq(StatusCode::NOT_FOUND));
    expect_that!(reset_opened.status, eq(StatusCode::NOT_FOUND));
    expect_that!(reset_used.status, eq(StatusCode::NOT_FOUND));
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
