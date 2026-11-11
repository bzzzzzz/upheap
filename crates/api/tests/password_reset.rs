//! Resetting a password with a link the operator made: what opening and using a link answers, what setting a
//! password ends, how links cancel one another, and what is recorded.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

const PASSWORD: &str = "a long test password";
const NEW_PASSWORD: &str = "another long password";
const EMAIL: &str = "ada@example.test";

async fn open(app: &TestApp, token: &str) -> Result<TestResponse> {
    app.send(Method::GET, &format!("/api/v1/password-resets/{token}"), None, None)
        .await
}

async fn reset(app: &TestApp, token: &str, password: &str) -> Result<TestResponse> {
    reset_as(app, None, token, password).await
}

/// Sets a password through the link, from a browser that holds `session`, if any.
async fn reset_as(app: &TestApp, session: Option<&str>, token: &str, password: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/password-resets/{token}"),
        session,
        Some(json!({ "password": password })),
    )
    .await
}

async fn sign_in(app: &TestApp, login: &str, password: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({ "login": login, "password": password })),
    )
    .await
}

async fn me(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(Method::GET, "/api/v1/me", Some(session), None).await
}

fn problem_code(response: &TestResponse) -> &str {
    response.body["code"].as_str().unwrap_or_default()
}

fn state(response: &TestResponse) -> &str {
    response.body["state"].as_str().unwrap_or_default()
}

/// A second browser session of the registered user.
async fn second_session(app: &TestApp) -> Result<String> {
    sign_in(app, EMAIL, PASSWORD)
        .await?
        .session_cookie
        .context("sign-in set no cookie")
}

#[gtest]
#[tokio::test]
async fn a_working_link_names_the_account_and_a_stranger_gets_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register_as(EMAIL, "Ada Lovelace").await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let opened = open(&app, &token).await?;
    let unknown = open(&app, &"a".repeat(43)).await?;
    let wrong_length = open(&app, "short").await?;
    let wrong_characters = open(&app, &format!("{}+", "a".repeat(42))).await?;

    // Assert
    expect_that!(opened.status, eq(StatusCode::OK));
    expect_that!(
        opened.body,
        eq(&json!({ "state": "working", "account": { "display_name": "Ada Lovelace", "username": "ada" } }))
    );
    expect_that!(unknown.status, eq(StatusCode::NOT_FOUND));
    expect_that!(wrong_length.status, eq(StatusCode::NOT_FOUND));
    expect_that!(wrong_characters.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_a_password_signs_in_and_ends_every_other_sign_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let other = second_session(&app).await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let done = reset(&app, &token, NEW_PASSWORD).await?;
    let new_session = done.session_cookie.clone().unwrap_or_default();
    let own = me(&app, &new_session).await?;
    let old_own = me(&app, &session).await?;
    let ended_session = me(&app, &other).await?;
    let ended_access = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;
    let refreshed = app
        .post_form(
            "/api/v1/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &refresh_token),
                ("client_id", common::CLI_CLIENT),
            ],
        )
        .await?;
    let old = sign_in(&app, EMAIL, PASSWORD).await?;
    let new = sign_in(&app, EMAIL, NEW_PASSWORD).await?;
    let opened_after = open(&app, &token).await?;
    let again = reset(&app, &token, "yet another password").await?;

    // Assert
    expect_that!(done.status, eq(StatusCode::OK));
    expect_that!(done.body["username"], eq(&json!("ada")));
    expect_that!(done.body["email"], eq(&json!(EMAIL)));
    expect_that!(own.status, eq(StatusCode::OK));
    expect_that!(old_own.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(ended_session.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(ended_access.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(refreshed.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(refreshed.body["error"], eq(&json!("invalid_grant")));
    expect_that!(old.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(new.status, eq(StatusCode::OK));
    expect_that!(state(&opened_after), eq("used"));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&again), eq("reset_link_used"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_newer_link_cancels_the_earlier_one() -> Result<()> {
    // Arrange: scenario 8.
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let first = app.make_reset_link("ada").await?;
    app.clock.advance(Duration::from_minutes(20));
    let second = app.make_reset_link("ada@example.test").await?;

    // Act
    let first_opened = open(&app, &first).await?;
    let first_used = reset(&app, &first, NEW_PASSWORD).await?;
    let second_opened = open(&app, &second).await?;
    let second_used = reset(&app, &second, NEW_PASSWORD).await?;

    // Assert
    expect_that!(state(&first_opened), eq("cancelled"));
    expect_that!(first_used.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&first_used), eq("reset_link_cancelled"));
    expect_that!(state(&second_opened), eq("working"));
    expect_that!(second_used.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_ends_after_sixty_minutes() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let token = app.make_reset_link("ada").await?;
    app.clock.advance(Duration::from_minutes(59));
    let just_before = open(&app, &token).await?;
    app.clock.advance(Duration::from_minutes(1));

    // Act
    let opened = open(&app, &token).await?;
    let used = reset(&app, &token, NEW_PASSWORD).await?;
    let old = sign_in(&app, EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(state(&just_before), eq("working"));
    expect_that!(state(&opened), eq("ended"));
    expect_that!(just_before.body["account"]["display_name"], eq(&json!("Tester")));
    expect_that!(opened.body["account"], eq(&json!(null)));
    expect_that!(used.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&used), eq("reset_link_ended"));
    expect_that!(old.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_password_change_cancels_the_working_link() -> Result<()> {
    // Arrange: scenario 9.
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let changed = app
        .send(
            Method::POST,
            "/api/v1/me/password",
            Some(&session),
            Some(json!({ "current_password": PASSWORD, "password": NEW_PASSWORD })),
        )
        .await?;
    let opened = open(&app, &token).await?;
    let used = reset(&app, &token, "yet another password").await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(state(&opened), eq("cancelled"));
    expect_that!(problem_code(&used), eq("reset_link_cancelled"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reset_starts_the_sign_in_attempts_over() -> Result<()> {
    // Arrange: scenario 10.
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let mut attempts = Vec::new();
    for _ in 0..6 {
        attempts.push(sign_in(&app, EMAIL, "not the password").await?.status);
    }
    let token = app.make_reset_link("ada").await?;

    // Act
    let locked = sign_in(&app, EMAIL, NEW_PASSWORD).await?;
    let done = reset(&app, &token, NEW_PASSWORD).await?;
    let signed_in = sign_in(&app, EMAIL, NEW_PASSWORD).await?;

    // Assert
    expect_that!(attempts.last(), some(eq(&StatusCode::TOO_MANY_REQUESTS)));
    expect_that!(locked.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(done.status, eq(StatusCode::OK));
    expect_that!(signed_in.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unacceptable_password_is_refused_and_the_link_keeps_working() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let empty = reset(&app, &token, "").await?;
    let short = reset(&app, &token, "nine char").await?;
    let long = reset(&app, &token, &"x".repeat(1025)).await?;
    let opened = open(&app, &token).await?;
    let done = reset(&app, &token, NEW_PASSWORD).await?;

    // Assert
    for (response, code) in [
        (&empty, "field_empty"),
        (&short, "password_too_short"),
        (&long, "password_too_long"),
    ] {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(problem_code(response), eq(code));
        expect_that!(response.body["field"], eq(&json!("password")));
    }
    expect_that!(state(&opened), eq("working"));
    expect_that!(done.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_old_password_is_accepted_as_the_new_one() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let done = reset(&app, &token, PASSWORD).await?;
    let signed_in = sign_in(&app, EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(done.status, eq(StatusCode::OK));
    expect_that!(signed_in.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_browser_signed_in_as_someone_else_is_signed_in_as_the_links_user() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let (bob_session, _) = app.register("bob@example.test").await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let opened = app
        .send(
            Method::GET,
            &format!("/api/v1/password-resets/{token}"),
            Some(&bob_session),
            None,
        )
        .await?;
    let done = reset_as(&app, Some(&bob_session), &token, NEW_PASSWORD).await?;
    let cookie = done.session_cookie.clone().unwrap_or_default();
    let own = me(&app, &cookie).await?;
    let bob_still = me(&app, &bob_session).await?;

    // Assert
    expect_that!(opened.body["account"]["username"], eq(&json!("ada")));
    expect_that!(done.status, eq(StatusCode::OK));
    expect_that!(cookie, not(eq(&bob_session)));
    expect_that!(own.body["user"]["username"], eq(&json!("ada")));
    expect_that!(bob_still.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn making_and_using_a_link_are_recorded_in_the_personal_team_without_secrets() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register_as(EMAIL, "Ada").await?;
    second_session(&app).await?;
    let user_id = me(&app, &session).await?.body["user"]["id"]
        .as_str()
        .context("no user")?
        .to_owned();
    let token = app.make_reset_link("ada").await?;

    // Act
    let done = reset(&app, &token, NEW_PASSWORD).await?;
    let new_session = done.session_cookie.context("the reset set no cookie")?;
    let entries = app.audit_entries(&user_id).await?;
    let made: Vec<&(String, Value)> = entries
        .iter()
        .filter(|(a, _)| a == "password_reset_link_made")
        .collect();
    let used: Vec<&(String, Value)> = entries.iter().filter(|(a, _)| a == "password_reset").collect();
    let team = me(&app, &new_session).await?.body["teams"][0]["id"]
        .as_str()
        .context("no team")?
        .to_owned();
    let listed = app
        .send(
            Method::GET,
            &format!("{}/audit", app.team_url(&team).await?),
            Some(&new_session),
            None,
        )
        .await?;

    // Assert
    expect_that!(made.len(), eq(1));
    expect_that!(made[0].1, eq(&json!({ "made_by": "operator" })));
    expect_that!(used.len(), eq(1));
    expect_that!(used[0].1, eq(&json!({ "ended_sign_ins": 2 })));
    let all = entries.iter().map(|(_, diff)| diff.to_string()).collect::<String>();
    expect_that!(all, not(contains_substring(token.as_str())));
    expect_that!(all, not(contains_substring(NEW_PASSWORD)));
    let phrases: Vec<&str> = listed.body["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["phrase"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(
        phrases,
        superset_of([
            &"was given a reset link by whoever runs the server",
            &"set a new password with a reset link"
        ])
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_endpoints_need_the_csrf_header() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;
    let token = app.make_reset_link("ada").await?;

    // Act
    let refused = app
        .send_with(
            Method::POST,
            &format!("/api/v1/password-resets/{token}"),
            None,
            Some(json!({ "password": NEW_PASSWORD })),
            &[],
        )
        .await?;
    let opened = open(&app, &token).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(state(&opened), eq("working"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_login_has_no_link() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register(EMAIL).await?;

    // Act
    let unknown = app.make_reset_link("nobody").await;

    // Assert
    expect_that!(unknown.is_err(), eq(true));
    Ok(())
}
