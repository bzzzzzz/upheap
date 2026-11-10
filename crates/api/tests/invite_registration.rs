//! Creating an account through an invite link: one act that makes the account, its personal team and heap and the
//! membership, with registration open or closed, and nothing when the link has stopped working.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

const PASSWORD: &str = "a long test password";

struct Setup {
    app: TestApp,
    ana: String,
    team_id: String,
    team_url: String,
}

/// Ana, admin of the shared team Ops.
async fn setup() -> Result<Setup> {
    let app = TestApp::start().await?;
    let (ana, _) = app.register_as("ana@example.test", "Ana").await?;
    let created = app
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    let team_id = created.body["id"].as_str().context("team has no id")?.to_owned();
    let team_url = app.team_url(&team_id).await?;
    Ok(Setup {
        app,
        ana,
        team_id,
        team_url,
    })
}

/// A new working link of Ops, made by Ana, as `(id, token)`.
async fn make_link(setup: &Setup) -> Result<(String, String)> {
    let made = setup
        .app
        .send(
            Method::POST,
            &format!("{}/invite-links", setup.team_url),
            Some(&setup.ana),
            None,
        )
        .await?;
    let id = made.body["id"].as_str().context("link has no id")?.to_owned();
    let token = made.body["token"].as_str().context("link has no token")?.to_owned();
    Ok((id, token))
}

async fn revoke(setup: &Setup, link_id: &str) -> Result<()> {
    let revoked = setup
        .app
        .send(
            Method::POST,
            &format!("{}/invite-links/{link_id}/revoke", setup.team_url),
            Some(&setup.ana),
            None,
        )
        .await?;
    anyhow::ensure!(revoked.status == StatusCode::OK, "revoke failed: {}", revoked.body);
    Ok(())
}

fn form(username: &str) -> Value {
    json!({
        "email": format!("{username}@example.test"),
        "username": username,
        "display_name": "Newcomer",
        "password": PASSWORD,
    })
}

async fn register_through(app: &TestApp, token: &str, body: Value) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/invites/{token}/register"),
        None,
        Some(body),
    )
    .await
}

async fn team_audit(setup: &Setup) -> Result<Vec<Value>> {
    let url = format!("{}/audit", setup.team_url);
    let log = setup.app.send(Method::GET, &url, Some(&setup.ana), None).await?;
    Ok(log.body["entries"].as_array().context("no entries")?.clone())
}

/// The counts of what registering makes, to show that a refusal made none of it.
async fn counts(app: &TestApp) -> Result<[i64; 4]> {
    Ok([
        app.count_rows("users").await?,
        app.count_rows("teams").await?,
        app.count_rows("team_members").await?,
        app.count_rows("audit_log").await?,
    ])
}

async fn assert_problem(response: &TestResponse, status: StatusCode, code: &str) {
    expect_that!(response.status, eq(status), "{}", response.body);
    expect_that!(response.body["code"], eq(&json!(code)));
}

async fn assert_registered_as_member(app: &TestApp, response: &TestResponse, username: &str) -> Result<()> {
    expect_that!(response.status, eq(StatusCode::CREATED), "{}", response.body);
    let cookie = response.session_cookie.as_deref().context("no session cookie")?;
    expect_that!(response.body["user"]["username"], eq(&json!(username)));
    expect_that!(response.body["team"]["name"], eq(&json!("Ops")));
    expect_that!(response.body["team"]["role"], eq(&json!("member")));
    expect_that!(
        response.body["team"]["members"]
            .as_array()
            .context("no members")?
            .iter()
            .filter(|m| m["username"] == json!(username))
            .count(),
        eq(1)
    );

    let me = app.send(Method::GET, "/api/v1/me", Some(cookie), None).await?;
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(me.body["user"]["username"], eq(&json!(username)));
    let personal = app
        .send(Method::GET, &format!("/api/v1/teams/{username}"), Some(cookie), None)
        .await?;
    expect_that!(personal.status, eq(StatusCode::OK));
    expect_that!(personal.body["personal"], eq(&json!(true)));
    expect_that!(personal.body["heaps"][0]["slug"], eq(&json!("personal")));
    let ops = app.send(Method::GET, "/api/v1/teams/ops", Some(cookie), None).await?;
    expect_that!(ops.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_newcomer_registers_through_a_link_on_a_closed_server() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;
    let closed = setup.app.with_registration_closed();

    // Act
    let registered = register_through(&closed, &token, form("nia")).await?;
    let plain = closed
        .send(Method::POST, "/api/v1/auth/register", None, Some(form("other")))
        .await?;

    // Assert
    assert_registered_as_member(&closed, &registered, "nia").await?;
    assert_problem(&plain, StatusCode::FORBIDDEN, "registration_closed").await;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_newcomer_registers_through_a_link_on_an_open_server() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;

    // Act
    let registered = register_through(&setup.app, &token, form("nia")).await?;

    // Assert
    assert_registered_as_member(&setup.app, &registered, "nia").await
}

#[gtest]
#[tokio::test]
async fn the_team_activity_has_one_new_entry_and_it_is_the_join() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;
    let before = team_audit(&setup).await?.len();
    setup.app.clock.advance(Duration::from_minutes(1));

    // Act
    let registered = register_through(&setup.app, &token, form("nia")).await?;

    // Assert
    expect_that!(registered.status, eq(StatusCode::CREATED));
    let entries = team_audit(&setup).await?;
    expect_that!(entries.len(), eq(before + 1));
    let entry = &entries[0];
    expect_that!(entry["action"], eq(&json!("member_joined")));
    expect_that!(entry["actor"]["display_name"], eq(&json!("Newcomer")));
    expect_that!(entry["entity"]["type"], eq(&json!("invite_link")));
    expect_that!(entry["entity"]["label"], eq(&json!("Ana's invite link")));
    let actions: Vec<&str> = entries.iter().filter_map(|e| e["action"].as_str()).collect();
    expect_that!(actions, not(contains(eq(&"user_registered"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_that_ended_while_the_form_was_open_makes_no_account() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let closed = setup.app.with_registration_closed();
    let (_, token) = make_link(&setup).await?;
    setup.app.clock.advance(Duration::from_days(6));
    setup.app.clock.advance(Duration::from_hours(23));
    setup.app.clock.advance(Duration::from_minutes(55));
    let opened = setup
        .app
        .send(Method::GET, &format!("/api/v1/invites/{token}"), None, None)
        .await?;
    setup.app.clock.advance(Duration::from_minutes(10));
    let before = counts(&setup.app).await?;

    // Act
    let late = register_through(&closed, &token, form("nia")).await?;
    let after = counts(&setup.app).await?;
    let (_, newer) = make_link(&setup).await?;
    let again = register_through(&closed, &newer, form("nia")).await?;

    // Assert
    expect_that!(opened.body["state"], eq(&json!("working")));
    assert_problem(&late, StatusCode::CONFLICT, "invite_link_ended").await;
    expect_that!(late.session_cookie, none());
    expect_that!(after, eq(before));
    assert_registered_as_member(&closed, &again, "nia").await
}

#[gtest]
#[tokio::test]
async fn a_revoked_link_makes_no_account() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (id, token) = make_link(&setup).await?;
    revoke(&setup, &id).await?;
    let before = counts(&setup.app).await?;

    // Act
    let refused = register_through(&setup.app, &token, form("nia")).await?;
    let after = counts(&setup.app).await?;
    let (_, working) = make_link(&setup).await?;
    let again = register_through(&setup.app, &working, form("nia")).await?;

    // Assert
    assert_problem(&refused, StatusCode::CONFLICT, "invite_link_revoked").await;
    expect_that!(after, eq(before));
    assert_registered_as_member(&setup.app, &again, "nia").await
}

#[gtest]
#[tokio::test]
async fn an_unknown_or_malformed_token_is_not_found_before_the_input_is_checked() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let invalid = json!({ "email": "nope", "username": "X", "display_name": "", "password": "x" });

    // Act
    let unknown = register_through(&setup.app, &"a".repeat(43), form("nia")).await?;
    let malformed = register_through(&setup.app, "short", invalid.clone()).await?;
    let unknown_invalid = register_through(&setup.app, &"a".repeat(43), invalid).await?;

    // Assert
    for response in [&unknown, &malformed, &unknown_invalid] {
        assert_problem(response, StatusCode::NOT_FOUND, "not_found").await;
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn input_errors_come_before_the_link_is_checked() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;
    let (id, revoked) = make_link(&setup).await?;
    revoke(&setup, &id).await?;
    let short = json!({ "email": "nia@example.test", "username": "nia", "display_name": "N", "password": "short" });
    let bad_username = json!({ "email": "nia@example.test", "username": "Not Valid!", "display_name": "N",
        "password": PASSWORD });

    // Act
    let short_password = register_through(&setup.app, &token, short.clone()).await?;
    let invalid_username = register_through(&setup.app, &token, bad_username).await?;
    let short_on_revoked = register_through(&setup.app, &revoked, short).await?;

    // Assert
    assert_problem(&short_password, StatusCode::UNPROCESSABLE_ENTITY, "password_too_short").await;
    assert_problem(&invalid_username, StatusCode::UNPROCESSABLE_ENTITY, "invalid_handle").await;
    expect_that!(short_on_revoked.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_email_or_username_is_refused_and_leaves_nothing() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;
    setup.app.register_as("sam@example.test", "Sam").await?;
    let before = counts(&setup.app).await?;
    let taken_email = json!({ "email": "sam@example.test", "username": "nia", "display_name": "N",
        "password": PASSWORD });
    let taken_username = json!({ "email": "nia@example.test", "username": "sam", "display_name": "N",
        "password": PASSWORD });

    // Act
    let email = register_through(&setup.app, &token, taken_email).await?;
    let username = register_through(&setup.app, &token, taken_username.clone()).await?;
    let after = counts(&setup.app).await?;

    // Assert
    assert_problem(&email, StatusCode::CONFLICT, "email_taken").await;
    expect_that!(email.body["field"], eq(&json!("email")));
    assert_problem(&username, StatusCode::CONFLICT, "handle_taken").await;
    expect_that!(username.body["field"], eq(&json!("username")));
    expect_that!(after, eq(before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_username_with_an_ended_link_says_the_link_ended() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;
    setup.app.register_as("sam@example.test", "Sam").await?;
    setup.app.clock.advance(Duration::from_days(7));

    // Act
    let refused = register_through(&setup.app, &token, form("sam")).await?;

    // Assert
    assert_problem(&refused, StatusCode::CONFLICT, "invite_link_ended").await;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stranger_removed_after_registering_keeps_their_account() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let closed = setup.app.with_registration_closed();
    let (id, token) = make_link(&setup).await?;
    let stranger = register_through(&closed, &token, form("eve")).await?;
    let eve_id = stranger.body["user"]["id"].as_str().context("no user id")?.to_owned();
    revoke(&setup, &id).await?;
    let member_url = setup.app.member_url(&setup.team_id, &eve_id).await?;
    let removed = setup
        .app
        .send(Method::DELETE, &member_url, Some(&setup.ana), None)
        .await?;
    anyhow::ensure!(removed.status == StatusCode::NO_CONTENT, "remove failed");

    // Act
    let login = closed
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "eve@example.test", "password": PASSWORD })),
        )
        .await?;
    let cookie = login.session_cookie.clone().context("no session cookie")?;
    let personal = closed
        .send(Method::GET, "/api/v1/teams/eve", Some(&cookie), None)
        .await?;
    let ops = closed
        .send(Method::GET, "/api/v1/teams/ops", Some(&cookie), None)
        .await?;
    let second = register_through(&closed, &token, form("fay")).await?;
    let rejoin = closed
        .send(
            Method::POST,
            &format!("/api/v1/invites/{token}/join"),
            Some(&cookie),
            None,
        )
        .await?;

    // Assert
    expect_that!(login.status, eq(StatusCode::OK));
    expect_that!(personal.status, eq(StatusCode::OK));
    expect_that!(ops.status, eq(StatusCode::NOT_FOUND));
    assert_problem(&second, StatusCode::CONFLICT, "invite_link_revoked").await;
    assert_problem(&rejoin, StatusCode::CONFLICT, "invite_link_revoked").await;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn two_people_register_through_one_link() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, token) = make_link(&setup).await?;

    // Act
    let first = register_through(&setup.app, &token, form("nia")).await?;
    let second = register_through(&setup.app, &token, form("ola")).await?;

    // Assert
    assert_registered_as_member(&setup.app, &first, "nia").await?;
    assert_registered_as_member(&setup.app, &second, "ola").await?;
    let ops = setup
        .app
        .send(Method::GET, &setup.team_url, Some(&setup.ana), None)
        .await?;
    expect_that!(ops.body["members"].as_array().map(Vec::len), some(eq(3)));
    Ok(())
}
