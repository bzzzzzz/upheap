//! Changing the password from the account page: what it checks, what it ends, how attempts are counted, and what
//! it records.

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

async fn change(app: &TestApp, session: &str, current: &str, new: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/me/password",
        Some(session),
        Some(json!({ "current_password": current, "password": new })),
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

/// A second browser session of the registered user.
async fn second_session(app: &TestApp) -> Result<String> {
    sign_in(app, EMAIL, PASSWORD)
        .await?
        .session_cookie
        .context("sign-in set no cookie")
}

async fn me(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(Method::GET, "/api/v1/me", Some(session), None).await
}

fn problem_code(response: &TestResponse) -> &str {
    response.body["code"].as_str().unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn a_change_ends_the_other_sessions_and_cli_sign_ins_and_keeps_the_callers() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let other = second_session(&app).await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;

    // Act
    let changed = change(&app, &session, PASSWORD, NEW_PASSWORD).await?;
    let own = me(&app, &session).await?;
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

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(changed.body, eq(&json!({ "ended_sign_ins": 2 })));
    expect_that!(own.status, eq(StatusCode::OK));
    expect_that!(ended_session.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(ended_access.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(refreshed.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(refreshed.body["error"], eq(&json!("invalid_grant")));
    expect_that!(old.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&old), eq("invalid_credentials"));
    expect_that!(new.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_change_with_no_other_sign_ins_ends_none() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let changed = change(&app, &session, PASSWORD, NEW_PASSWORD).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(changed.body, eq(&json!({ "ended_sign_ins": 0 })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sign_ins_that_were_not_live_are_not_counted() -> Result<()> {
    // Arrange: one session expires, and one CLI sign-in is ended by the user.
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let expired = second_session(&app).await?;
    app.sign_in_cli(&session).await?;
    let id = app
        .send(Method::GET, "/api/v1/me/sign-ins", Some(&session), None)
        .await?
        .body["sign_ins"][0]["id"]
        .as_str()
        .context("no sign-in")?
        .to_owned();
    app.send(
        Method::DELETE,
        &format!("/api/v1/me/sign-ins/{id}"),
        Some(&session),
        None,
    )
    .await?;
    app.clock.advance(Duration::from_days(31));
    let caller = second_session(&app).await?;

    // Act
    let changed = change(&app, &caller, PASSWORD, NEW_PASSWORD).await?;
    let expired_after = me(&app, &expired).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(changed.body, eq(&json!({ "ended_sign_ins": 0 })));
    expect_that!(expired_after.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_wrong_current_password_is_refused_and_changes_nothing() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let other = second_session(&app).await?;

    // Act
    let refused = change(&app, &session, "not the password", NEW_PASSWORD).await?;
    let other_after = me(&app, &other).await?;
    let old = sign_in(&app, EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(problem_code(&refused), eq("wrong_password"));
    expect_that!(refused.body["field"], eq(&json!("current_password")));
    expect_that!(other_after.status, eq(StatusCode::OK));
    expect_that!(old.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn attempts_are_counted_with_sign_in_before_the_password_is_checked() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    for _ in 0..5 {
        change(&app, &session, "not the password", NEW_PASSWORD).await?;
    }

    // Act
    let refused = change(&app, &session, PASSWORD, NEW_PASSWORD).await?;
    let signed_in = sign_in(&app, EMAIL, PASSWORD).await?;
    let own = me(&app, &session).await?;
    app.clock.advance(Duration::from_minutes(15));
    let old_after_window = sign_in(&app, EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(problem_code(&refused), eq("too_many_attempts"));
    expect_that!(refused.retry_after, some(anything()));
    expect_that!(signed_in.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(own.status, eq(StatusCode::OK));
    expect_that!(old_after_window.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn wrong_sign_ins_count_against_a_change_too() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    for _ in 0..5 {
        sign_in(&app, EMAIL, "not the password").await?;
    }

    // Act
    let refused = change(&app, &session, PASSWORD, NEW_PASSWORD).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::TOO_MANY_REQUESTS));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_successful_change_clears_the_count() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    change(&app, &session, "not the password", NEW_PASSWORD).await?;

    // Act
    let changed = change(&app, &session, PASSWORD, NEW_PASSWORD).await?;
    let mut wrong = Vec::new();
    for _ in 0..5 {
        wrong.push(sign_in(&app, EMAIL, "not the password").await?.status);
    }
    let sixth = sign_in(&app, EMAIL, "not the password").await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(wrong, each(eq(&StatusCode::UNAUTHORIZED)));
    expect_that!(sixth.status, eq(StatusCode::TOO_MANY_REQUESTS));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn empty_and_short_input_is_refused_without_using_an_attempt() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let no_current = change(&app, &session, "", NEW_PASSWORD).await?;
    let no_new = change(&app, &session, PASSWORD, "").await?;
    let short = change(&app, &session, PASSWORD, "nine char").await?;
    for _ in 0..3 {
        change(&app, &session, "", NEW_PASSWORD).await?;
    }
    let changed = change(&app, &session, PASSWORD, NEW_PASSWORD).await?;

    // Assert
    for (response, code, field) in [
        (&no_current, "field_empty", "current_password"),
        (&no_new, "field_empty", "password"),
        (&short, "password_too_short", "password"),
    ] {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(problem_code(response), eq(code));
        expect_that!(response.body["field"], eq(&json!(field)));
    }
    expect_that!(changed.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bearer_token_cannot_change_the_password() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let (access, _) = app.sign_in_cli(&session).await?;

    // Act
    let refused = app
        .send_bearer(
            Method::POST,
            "/api/v1/me/password",
            &access,
            Some(json!({ "current_password": PASSWORD, "password": NEW_PASSWORD })),
        )
        .await?;
    let old = sign_in(&app, EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&refused), eq("unauthenticated"));
    expect_that!(old.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_same_password_is_accepted_and_still_ends_the_other_sessions() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let other = second_session(&app).await?;

    // Act
    let changed = change(&app, &session, PASSWORD, PASSWORD).await?;
    let other_after = me(&app, &other).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(changed.body, eq(&json!({ "ended_sign_ins": 1 })));
    expect_that!(other_after.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_change_is_recorded_in_the_personal_team_only() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register_as(EMAIL, "Ada").await?;
    let (shared_team, _) = app.shared_heap(&session).await?;
    let own = me(&app, &session).await?;
    let personal_team = own.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    let user_id = own.body["user"]["id"].as_str().context("no user")?.to_owned();

    // Act
    change(&app, &session, PASSWORD, NEW_PASSWORD).await?;
    let personal = app
        .send(
            Method::GET,
            &format!("{}/audit", app.team_url(&personal_team).await?),
            Some(&session),
            None,
        )
        .await?;
    let shared = app
        .send(
            Method::GET,
            &format!("{}/audit", app.team_url(&shared_team).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    let entry = &personal.body["entries"][0];
    expect_that!(entry["action"], eq(&json!("password_changed")));
    expect_that!(entry["actor"]["id"], eq(&json!(user_id)));
    expect_that!(entry["phrase"], eq(&json!("changed their password")));
    expect_that!(entry["names_entity"], eq(&json!(false)));
    expect_that!(entry["diff"], eq(&json!({ "ended_sign_ins": 0 })));
    expect_that!(entry.to_string(), not(contains_substring(NEW_PASSWORD)));
    let shared_actions: Vec<&str> = shared.body["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["action"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(shared_actions, not(contains(eq(&"password_changed"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn me_offers_changing_the_password() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let me: Value = me(&app, &session).await?.body;

    // Assert
    expect_that!(
        me["allowed_actions"],
        eq(&json!([
            "sign_out",
            "create_team",
            "manage_sign_ins",
            "change_password"
        ]))
    );
    Ok(())
}
