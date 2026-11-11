//! The account history on `GET /me/history`: the caller's password changes, reset links made for them and passwords
//! set with a link, worded by the server, newest first, and nobody else's.

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

async fn history(app: &TestApp, session: &str, query: &str) -> Result<TestResponse> {
    app.send(Method::GET, &format!("/api/v1/me/history{query}"), Some(session), None)
        .await
}

async fn change(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/me/password",
        Some(session),
        Some(json!({ "current_password": PASSWORD, "password": NEW_PASSWORD })),
    )
    .await
}

async fn reset(app: &TestApp, token: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/password-resets/{token}"),
        None,
        Some(json!({ "password": PASSWORD })),
    )
    .await
}

fn entries(response: &TestResponse) -> &[Value] {
    response.body["entries"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn field<'a>(entries: &'a [Value], name: &str) -> Vec<&'a str> {
    entries.iter().filter_map(|entry| entry[name].as_str()).collect()
}

#[gtest]
#[tokio::test]
async fn a_new_user_has_no_history() -> Result<()> {
    // Arrange: signing up and the profile are recorded in the same team, and aren't part of the history.
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    app.patch(
        &session,
        "/api/v1/me",
        &me.body["user"]["version"],
        json!({ "display_name": "Ada" }),
    )
    .await?;

    // Act
    let listed = history(&app, &session, "").await?;

    // Assert
    expect_that!(listed.status, eq(StatusCode::OK));
    expect_that!(listed.body, eq(&json!({ "entries": [], "next_cursor": null })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_password_change_is_listed_with_the_number_of_other_sign_ins_it_ended() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    app.sign_in_cli(&session).await?;

    // Act
    let changed = change(&app, &session).await?;
    let new_session = changed.session_cookie.context("the change set no cookie")?;
    let listed = history(&app, &new_session, "").await?;

    // Assert
    let listed_entries = entries(&listed);
    expect_that!(listed_entries.len(), eq(1));
    expect_that!(listed_entries[0]["kind"], eq(&json!("password_changed")));
    expect_that!(
        listed_entries[0]["description"],
        eq(&json!("Password changed. 1 other sign-in ended."))
    );
    expect_that!(listed_entries[0]["created_at"], eq(&json!(common::START)));
    expect_that!(listed.body["next_cursor"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_change_with_no_other_sign_ins_says_so() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let new_session = change(&app, &session).await?.session_cookie.context("no cookie")?;
    let listed = history(&app, &new_session, "").await?;

    // Assert
    expect_that!(
        field(entries(&listed), "description"),
        elements_are![eq(&"Password changed. No other sign-ins were open.")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reset_link_and_its_use_are_listed_newest_first() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let second = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "login": EMAIL, "password": PASSWORD })),
        )
        .await?
        .session_cookie
        .context("no cookie")?;
    let new_session = change(&app, &session).await?.session_cookie.context("no cookie")?;
    app.clock.advance(Duration::from_minutes(5));
    let token = app.make_reset_link("ada").await?;
    app.clock.advance(Duration::from_minutes(5));

    // Act
    let before_use = history(&app, &new_session, "").await?;
    let used = reset(&app, &token).await?;
    let after_use = history(&app, used.session_cookie.as_deref().unwrap_or_default(), "").await?;
    let ended = history(&app, &second, "").await?;

    // Assert
    expect_that!(
        field(entries(&before_use), "kind"),
        elements_are![eq(&"password_reset_link_made"), eq(&"password_changed")]
    );
    expect_that!(
        field(entries(&after_use), "kind"),
        elements_are![
            eq(&"password_reset"),
            eq(&"password_reset_link_made"),
            eq(&"password_changed")
        ]
    );
    expect_that!(
        field(entries(&after_use), "description"),
        elements_are![
            eq(&"Password set with a reset link. 1 sign-in ended."),
            eq(&"Reset link made by whoever runs the server."),
            eq(&"Password changed. 1 other sign-in ended.")
        ]
    );
    expect_that!(ended.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn another_users_events_are_not_listed() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register(EMAIL).await?;
    let (bob, _) = app.register("bob@example.test").await?;
    change(&app, &bob).await?;
    app.make_reset_link("bob").await?;

    // Act
    let listed = history(&app, &ada, "").await?;

    // Assert
    expect_that!(listed.status, eq(StatusCode::OK));
    expect_that!(entries(&listed), is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn pages_continue_with_the_cursor_without_repeats() -> Result<()> {
    // Arrange: three events at different times.
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    app.make_reset_link("ada").await?;
    app.clock.advance(Duration::from_minutes(1));
    app.make_reset_link("ada").await?;
    app.clock.advance(Duration::from_minutes(1));
    let session = change(&app, &session).await?.session_cookie.context("no cookie")?;

    // Act
    let first = history(&app, &session, "?limit=1").await?;
    let cursor = |page: &TestResponse| page.body["next_cursor"].as_str().map(str::to_owned);
    let second = history(
        &app,
        &session,
        &format!("?limit=1&cursor={}", cursor(&first).unwrap_or_default()),
    )
    .await?;
    let third = history(
        &app,
        &session,
        &format!("?limit=1&cursor={}", cursor(&second).unwrap_or_default()),
    )
    .await?;
    let bad = history(&app, &session, "?cursor=garbage").await?;

    // Assert
    expect_that!(field(entries(&first), "kind"), elements_are![eq(&"password_changed")]);
    expect_that!(cursor(&first), some(anything()));
    expect_that!(
        field(entries(&second), "kind"),
        elements_are![eq(&"password_reset_link_made")]
    );
    expect_that!(
        field(entries(&third), "kind"),
        elements_are![eq(&"password_reset_link_made")]
    );
    let ids = [&first, &second, &third].map(|page| field(entries(page), "id").join(""));
    expect_that!(ids[0], not(eq(&ids[1])));
    expect_that!(ids[1], not(eq(&ids[2])));
    expect_that!(third.body["next_cursor"], eq(&Value::Null));
    expect_that!(bad.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(bad.body["code"], eq(&json!("invalid_cursor")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn it_needs_a_sign_in_and_a_cli_token_may_read_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    app.make_reset_link("ada").await?;
    let (access, _) = app.sign_in_cli(&session).await?;

    // Act
    let signed_out = app.send(Method::GET, "/api/v1/me/history", None, None).await?;
    let with_token = app
        .send_bearer(Method::GET, "/api/v1/me/history", &access, None)
        .await?;

    // Assert
    expect_that!(signed_out.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(with_token.status, eq(StatusCode::OK));
    expect_that!(entries(&with_token).len(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_time_label_is_in_the_requests_time_zone() -> Result<()> {
    // Arrange: the test clock starts at 12:00 UTC on 1 Jan 2027; the harness sends Europe/Berlin.
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    app.make_reset_link("ada").await?;

    // Act
    let listed = history(&app, &session, "").await?;

    // Assert
    expect_that!(
        field(entries(&listed), "created_label"),
        elements_are![eq(&"1 Jan 2027, 13:00")]
    );
    Ok(())
}
