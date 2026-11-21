//! Changing an email address with a mailed link: asking, what the mails say, applying, and what is recorded.

mod common;

use std::path::Path;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

const PASSWORD: &str = "a long test password";
const EMAIL: &str = "ada@example.test";
const NEW_EMAIL: &str = "grace@example.test";

async fn ask_as(app: &TestApp, session: &str, email: &str, password: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/me/email-changes",
        Some(session),
        Some(json!({ "email": email, "current_password": password })),
    )
    .await
}

async fn apply(app: &TestApp, token: &str) -> Result<TestResponse> {
    app.send(Method::POST, &format!("/api/v1/email-changes/{token}"), None, None)
        .await
}

async fn me(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(Method::GET, "/api/v1/me", Some(session), None).await
}

async fn sign_in(app: &TestApp, login: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({ "login": login, "password": PASSWORD })),
    )
    .await
}

fn problem_code(response: &TestResponse) -> &str {
    response.body["code"].as_str().unwrap_or_default()
}

/// The email change token in a mail's text.
fn token_in(mail: &str) -> Result<String> {
    let (_, after) = mail.split_once("/change-email/").context("no link in the mail")?;
    Ok(after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect())
}

/// A server with mail, written to `dir`, and the registered user's session.
async fn start(dir: &Path) -> Result<(TestApp, String)> {
    let (app, session, _) = start_with_plain(dir).await?;
    Ok((app, session))
}

/// [`start`], and the same server without mail too, for the accounts a test needs that nobody has to confirm: a
/// server with mail makes an account that signs in only once its address is confirmed.
async fn start_with_plain(dir: &Path) -> Result<(TestApp, String, TestApp)> {
    let plain = TestApp::start().await?;
    let (session, _) = plain.register(EMAIL).await?;
    Ok((plain.with_mail(dir)?, session, plain))
}

fn actions(response: &TestResponse) -> Vec<&str> {
    response.body["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn asking_mails_a_link_to_the_new_address_and_changes_nothing() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;

    // Act
    let asked = ask_as(&app, &session, "  Grace@Example.test ", PASSWORD).await?;
    let mails = common::wait_for_mails(dir.path(), 1).await?;
    let current = me(&app, &session).await?;

    // Assert
    expect_that!(asked.status, eq(StatusCode::ACCEPTED));
    expect_that!(asked.body, eq(&Value::Null));
    expect_that!(mails.len(), eq(1));
    expect_that!(mails[0], contains_substring("To: grace@example.test"));
    expect_that!(mails[0], contains_substring("Subject: Confirm your new Upheap address"));
    expect_that!(
        mails[0],
        contains_substring("@ada asked to use this address for their account on upheap.example.test.")
    );
    expect_that!(
        mails[0],
        contains_substring("https://upheap.example.test/change-email/")
    );
    expect_that!(current.body["user"]["email"], eq(&json!(EMAIL)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn applying_changes_the_address_and_tells_the_old_one() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;
    let other_session = sign_in(&app, EMAIL).await?.session_cookie.context("no cookie")?;
    let version_before = me(&app, &session).await?.body["user"]["version"].as_i64();
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let token = token_in(&common::wait_for_mails(dir.path(), 1).await?[0])?;

    // Act
    let applied = apply(&app, &token).await?;
    let mails = common::wait_for_mails(dir.path(), 2).await?;
    let current = me(&app, &session).await?;
    let other = me(&app, &other_session).await?;
    let new_sign_in = sign_in(&app, NEW_EMAIL).await?;
    let old_sign_in = sign_in(&app, EMAIL).await?;
    let again = apply(&app, &token).await?;

    // Assert
    expect_that!(applied.status, eq(StatusCode::OK));
    expect_that!(applied.body, eq(&json!({ "email": NEW_EMAIL })));
    expect_that!(current.body["user"]["email"], eq(&json!(NEW_EMAIL)));
    expect_that!(
        current.body["user"]["version"].as_i64(),
        some(eq(version_before.unwrap_or_default() + 1))
    );
    expect_that!(mails[1], contains_substring("To: ada@example.test"));
    expect_that!(mails[1], contains_substring("Subject: Your Upheap address changed"));
    expect_that!(
        mails[1],
        contains_substring(
            "The address for @ada on upheap.example.test changed from ada@example.test to grace@example.test."
        )
    );
    expect_that!(other.status, eq(StatusCode::OK));
    expect_that!(new_sign_in.status, eq(StatusCode::OK));
    expect_that!(old_sign_in.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&again), eq("email_change_used"));
    expect_that!(again.body["title"], eq(&json!("This link was already used.")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_wrong_password_sends_nothing() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;

    // Act
    let wrong = ask_as(&app, &session, NEW_EMAIL, "not the password").await?;
    let empty = ask_as(&app, &session, NEW_EMAIL, "").await?;
    let right = ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let mails = common::wait_for_mails(dir.path(), 1).await?;

    // Assert: only the right password got a mail, and the one mail is the right one's.
    expect_that!(wrong.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(problem_code(&wrong), eq("wrong_password"));
    expect_that!(wrong.body["field"], eq(&json!("current_password")));
    expect_that!(problem_code(&empty), eq("field_empty"));
    expect_that!(empty.body["field"], eq(&json!("current_password")));
    expect_that!(right.status, eq(StatusCode::ACCEPTED));
    expect_that!(mails.len(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_address_gets_the_same_answer_and_no_mail() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session, plain) = start_with_plain(dir.path()).await?;
    plain.register("grace@example.test").await?;

    // Act
    let taken = ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let free = ask_as(&app, &session, "hopper@example.test", PASSWORD).await?;
    let mails = common::wait_for_mails(dir.path(), 1).await?;
    let history = app
        .send(Method::GET, "/api/v1/me/history", Some(&session), None)
        .await?;

    // Assert
    expect_that!(taken.status, eq(StatusCode::ACCEPTED));
    expect_that!(taken.body, eq(&free.body));
    expect_that!(free.status, eq(taken.status));
    expect_that!(mails.len(), eq(1));
    expect_that!(mails[0], contains_substring("To: hopper@example.test"));
    let descriptions: Vec<_> = history.body["entries"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry["description"].as_str())
                .collect()
        })
        .unwrap_or_default();
    expect_that!(
        descriptions,
        elements_are![
            eq(&"Address change to hopper@example.test asked. A link was mailed there."),
            eq(&"Address change to grace@example.test asked. A link was mailed there.")
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_invalid_address_is_refused_before_anything_is_counted() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;

    // Act
    let invalid = ask_as(&app, &session, "not an address", PASSWORD).await?;
    let empty = ask_as(&app, &session, "", PASSWORD).await?;

    // Assert
    expect_that!(invalid.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(problem_code(&invalid), eq("email_invalid"));
    expect_that!(invalid.body["field"], eq(&json!("email")));
    expect_that!(problem_code(&empty), eq("field_empty"));
    expect_that!(empty.body["field"], eq(&json!("email")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_action_needs_mail_a_session_and_a_password() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let plain = TestApp::start().await?;
    let (plain_session, _) = plain.register(EMAIL).await?;
    let (app, session) = start(dir.path()).await?;
    let token = app.make_personal_token(&session, "cli", None).await?.body["secret"]
        .as_str()
        .context("no token")?
        .to_owned();

    // Act
    let without_mail = ask_as(&plain, &plain_session, NEW_EMAIL, PASSWORD).await?;
    let me_without_mail = me(&plain, &plain_session).await?;
    let me_with_mail = me(&app, &session).await?;
    let bearer = app
        .send_bearer(
            Method::POST,
            "/api/v1/me/email-changes",
            &token,
            Some(json!({ "email": NEW_EMAIL, "current_password": PASSWORD })),
        )
        .await?;

    // Assert
    expect_that!(without_mail.status, eq(StatusCode::NOT_FOUND));
    expect_that!(actions(&me_without_mail), not(contains(eq(&"change_email"))));
    expect_that!(actions(&me_with_mail), contains(eq(&"change_email")));
    expect_that!(bearer.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_ask_for_a_taken_address_cancels_the_earlier_link() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session, plain) = start_with_plain(dir.path()).await?;
    plain.register("grace@example.test").await?;
    ask_as(&app, &session, "hopper@example.test", PASSWORD).await?;
    let first = token_in(&common::wait_for_mails(dir.path(), 1).await?[0])?;
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;

    // Act
    let applied = apply(&app, &first).await?;

    // Assert
    expect_that!(applied.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&applied), eq("email_change_cancelled"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_newer_ask_cancels_the_earlier_link() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    ask_as(&app, &session, "hopper@example.test", PASSWORD).await?;
    let mails = common::wait_for_mails(dir.path(), 2).await?;
    let (first, second) = (token_in(&mails[0])?, token_in(&mails[1])?);

    // Act
    let first_applied = apply(&app, &first).await?;
    let second_applied = apply(&app, &second).await?;

    // Assert
    expect_that!(first_applied.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&first_applied), eq("email_change_cancelled"));
    expect_that!(
        first_applied.body["title"],
        eq(&json!("This link no longer works: a newer one was asked for."))
    );
    expect_that!(second_applied.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_ended_link_says_so() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let token = token_in(&common::wait_for_mails(dir.path(), 1).await?[0])?;
    app.clock.advance(domain::Duration::from_hours(24));

    // Act
    let applied = apply(&app, &token).await?;
    let unknown = apply(&app, &"a".repeat(43)).await?;
    let malformed = apply(&app, "short").await?;

    // Assert
    expect_that!(applied.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&applied), eq("email_change_ended"));
    expect_that!(unknown.status, eq(StatusCode::NOT_FOUND));
    expect_that!(malformed.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_address_taken_since_the_ask_is_refused_and_the_link_keeps_working() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session, plain) = start_with_plain(dir.path()).await?;
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let token = token_in(&common::wait_for_mails(dir.path(), 1).await?[0])?;
    let (grace_session, _) = plain.register(NEW_EMAIL).await?;

    // Act
    let refused = apply(&app, &token).await?;
    let current = me(&app, &session).await?;
    // Grace changes her own address away, so the link's address is free again.
    app.send(
        Method::POST,
        "/api/v1/me/email-changes",
        Some(&grace_session),
        Some(json!({ "email": "other@example.test", "current_password": PASSWORD })),
    )
    .await?;
    let grace_mail = common::wait_for_mails(dir.path(), 2).await?;
    let grace_token = token_in(&grace_mail[1])?;
    apply(&app, &grace_token).await?;
    let applied = apply(&app, &token).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&refused), eq("email_taken"));
    expect_that!(current.body["user"]["email"], eq(&json!(EMAIL)));
    expect_that!(applied.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn applying_cancels_a_working_reset_link() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;
    let reset_token = app.make_reset_link("ada").await?;
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let token = token_in(&common::wait_for_mails(dir.path(), 1).await?[0])?;

    // Act
    apply(&app, &token).await?;
    let opened = app
        .send(
            Method::GET,
            &format!("/api/v1/password-resets/{reset_token}"),
            None,
            None,
        )
        .await?;

    // Assert
    expect_that!(opened.body["state"], eq(&json!("cancelled")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_sixth_ask_of_a_session_is_refused() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;
    for _ in 0..5 {
        let asked = ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
        assert_eq!(asked.status, StatusCode::ACCEPTED);
    }

    // Act
    let sixth = ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(sixth.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(problem_code(&sixth), eq("too_many_attempts"));
    expect_that!(sixth.headers.contains_key("retry-after"), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn new_sessions_dont_start_the_accounts_address_changes_over() -> Result<()> {
    // Arrange: five asks, each from a session of its own.
    let dir = tempfile::tempdir()?;
    let (app, first) = start(dir.path()).await?;
    let mut sessions = vec![first];
    for _ in 0..5 {
        let signed_in = sign_in(&app, EMAIL).await?;
        sessions.push(signed_in.session_cookie.clone().context("no session cookie")?);
    }
    for (n, session) in sessions[..5].iter().enumerate() {
        let asked = ask_as(&app, session, &format!("grace{n}@example.test"), PASSWORD).await?;
        assert_eq!(asked.status, StatusCode::ACCEPTED);
    }
    common::wait_for_mails(dir.path(), 5).await?;

    // Act
    let sixth = ask_as(&app, &sessions[5], "grace5@example.test", PASSWORD).await?;

    // Assert
    expect_that!(sixth.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(problem_code(&sixth), eq("too_many_attempts"));
    expect_that!(sixth.headers.contains_key("retry-after"), eq(true));
    expect_that!(std::fs::read_dir(dir.path())?.count(), eq(5));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_history_lists_the_ask_and_the_change() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let (app, session) = start(dir.path()).await?;
    ask_as(&app, &session, NEW_EMAIL, PASSWORD).await?;
    let token = token_in(&common::wait_for_mails(dir.path(), 1).await?[0])?;
    apply(&app, &token).await?;

    // Act
    let history = app
        .send(Method::GET, "/api/v1/me/history", Some(&session), None)
        .await?;

    // Assert
    let entries = history.body["entries"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<_> = entries.iter().filter_map(|entry| entry["kind"].as_str()).collect();
    let descriptions: Vec<_> = entries
        .iter()
        .filter_map(|entry| entry["description"].as_str())
        .collect();
    expect_that!(kinds, elements_are![eq(&"email_changed"), eq(&"email_change_asked")]);
    expect_that!(
        descriptions,
        elements_are![
            eq(&"Address changed from ada@example.test to grace@example.test."),
            eq(&"Address change to grace@example.test asked. A link was mailed there.")
        ]
    );
    Ok(())
}
