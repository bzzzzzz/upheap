//! Confirming an address before sign-in, on a server with mail set up: registering, the refused sign-in and its new
//! link, the confirm endpoint, the other ways to confirm, and what is recorded.

mod common;

use std::path::Path;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

const PASSWORD: &str = "a long test password";
const EMAIL: &str = "ada@example.test";

fn body(email: &str) -> Value {
    json!({
        "email": email,
        "username": common::username_for(email),
        "display_name": "Ada",
        "password": PASSWORD,
    })
}

async fn register(app: &TestApp, email: &str) -> Result<TestResponse> {
    app.send(Method::POST, "/api/v1/auth/register", None, Some(body(email)))
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

async fn confirm(app: &TestApp, token: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/email-confirmations/{token}"),
        None,
        None,
    )
    .await
}

async fn me(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(Method::GET, "/api/v1/me", Some(session), None).await
}

fn problem_code(response: &TestResponse) -> &str {
    response.body["code"].as_str().unwrap_or_default()
}

/// The confirmation token in a mail's text.
fn token_in(mail: &str) -> Result<String> {
    let (_, after) = mail.split_once("/confirm/").context("no link in the mail")?;
    Ok(after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect())
}

/// The token of the `n`th mail (1 for the first) to arrive in `dir`.
async fn token_of_mail(dir: &Path, n: usize) -> Result<String> {
    token_in(&common::wait_for_mails(dir, n).await?[n - 1])
}

#[gtest]
#[tokio::test]
async fn an_account_confirms_before_it_signs_in_and_the_link_works_once() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;

    // Act
    let registered = register(&app, EMAIL).await?;
    let mails = common::wait_for_mails(dir.path(), 1).await?;
    let first = token_in(&mails[0])?;
    let refused = sign_in(&app, EMAIL, PASSWORD).await?;
    // Three more presses of "Send it again", each a counted sign-in, then a wrong password: the fifth attempt.
    for _ in 0..3 {
        sign_in(&app, EMAIL, PASSWORD).await?;
    }
    let wrong = sign_in(&app, EMAIL, "not the password").await?;
    // Each refused sign-in mailed a new link, which cancelled the one before.
    let token = token_of_mail(dir.path(), 5).await?;
    let cancelled = confirm(&app, &first).await?;
    let confirmed = confirm(&app, &token).await?;
    let cookie = confirmed.session_cookie.clone().context("no session cookie")?;
    let current = me(&app, &cookie).await?;
    // Confirming cleared the count, so this sixth attempt isn't refused as too many.
    let signed_in = sign_in(&app, EMAIL, PASSWORD).await?;
    let again = confirm(&app, &token).await?;

    // Assert
    expect_that!(registered.status, eq(StatusCode::ACCEPTED), "{}", registered.body);
    expect_that!(registered.session_cookie, none());
    expect_that!(registered.body, eq(&json!({ "email": EMAIL })));
    expect_that!(mails[0], contains_substring("To: ada@example.test"));
    expect_that!(mails[0], contains_substring("Subject: Confirm your Upheap address"));
    expect_that!(
        mails[0],
        contains_substring("Someone made the account @ada on upheap.example.test with this address.")
    );
    expect_that!(mails[0], contains_substring("https://upheap.example.test/confirm/"));
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(problem_code(&refused), eq("email_not_confirmed"));
    expect_that!(
        refused.body["title"],
        eq(&json!("Confirm your address first: we mailed you a new link."))
    );
    expect_that!(refused.session_cookie, none());
    expect_that!(wrong.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&wrong), eq("invalid_credentials"));
    expect_that!(problem_code(&cancelled), eq("confirmation_link_cancelled"));
    expect_that!(confirmed.status, eq(StatusCode::OK), "{}", confirmed.body);
    expect_that!(confirmed.body["email"], eq(&json!(EMAIL)));
    expect_that!(current.status, eq(StatusCode::OK));
    expect_that!(current.body["user"]["username"], eq(&json!("ada")));
    expect_that!(signed_in.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&again), eq("confirmation_link_used"));
    expect_that!(again.body["title"], eq(&json!("This link was already used.")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_refused_sign_in_mails_a_new_link_and_cancels_the_earlier_one() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;
    let first = token_of_mail(dir.path(), 1).await?;

    // Act
    let refused = sign_in(&app, "ada", PASSWORD).await?;
    let second = token_of_mail(dir.path(), 2).await?;
    let old = confirm(&app, &first).await?;
    let new = confirm(&app, &second).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(first, not(eq(&second)));
    expect_that!(old.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&old), eq("confirmation_link_cancelled"));
    expect_that!(
        old.body["title"],
        eq(&json!("This link no longer works: a newer one was asked for."))
    );
    expect_that!(new.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_address_is_refused_as_before_and_mails_nothing() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;
    common::wait_for_mails(dir.path(), 1).await?;

    // Act
    let taken = register(&app, EMAIL).await?;

    // Assert
    expect_that!(taken.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&taken), eq("email_taken"));
    expect_that!(std::fs::read_dir(dir.path())?.count(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn registering_through_an_invite_waits_for_the_confirmation_and_then_the_user_is_a_member() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let plain = TestApp::start().await?;
    let (ana, _) = plain.register_as("ana@example.test", "Ana").await?;
    let app = plain.with_mail(dir.path())?;
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
    let link = app
        .send(Method::POST, &format!("{team_url}/invite-links"), Some(&ana), None)
        .await?;
    let invite = link.body["token"].as_str().context("link has no token")?.to_owned();

    // Act
    let registered = app
        .send(
            Method::POST,
            &format!("/api/v1/invites/{invite}/register"),
            None,
            Some(body(EMAIL)),
        )
        .await?;
    let token = token_of_mail(dir.path(), 1).await?;
    let confirmed = confirm(&app, &token).await?;
    let cookie = confirmed.session_cookie.context("no session cookie")?;
    let ops = app.send(Method::GET, "/api/v1/teams/ops", Some(&cookie), None).await?;

    // Assert
    expect_that!(registered.status, eq(StatusCode::ACCEPTED), "{}", registered.body);
    expect_that!(registered.session_cookie, none());
    expect_that!(registered.body, eq(&json!({ "email": EMAIL })));
    expect_that!(ops.status, eq(StatusCode::OK), "{}", ops.body);
    expect_that!(ops.body["role"], eq(&json!("member")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_ends_after_a_day_and_an_unknown_token_is_not_found() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;
    let token = token_of_mail(dir.path(), 1).await?;

    // Act
    app.clock.advance(domain::Duration::from_hours(24));
    let late = confirm(&app, &token).await?;
    let unknown = confirm(&app, &"a".repeat(43)).await?;
    let malformed = confirm(&app, "short").await?;

    // Assert
    expect_that!(late.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&late), eq("confirmation_link_ended"));
    expect_that!(late.body["title"], eq(&json!("This link has ended.")));
    expect_that!(unknown.status, eq(StatusCode::NOT_FOUND));
    expect_that!(malformed.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_operators_reset_link_confirms_the_address_too() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;
    let reset = app.make_reset_link("ada").await?;

    // Act
    let set = app
        .send(
            Method::POST,
            &format!("/api/v1/password-resets/{reset}"),
            None,
            Some(json!({ "password": "another long password" })),
        )
        .await?;
    let signed_in = sign_in(&app, EMAIL, "another long password").await?;

    // Assert
    expect_that!(set.status, eq(StatusCode::OK), "{}", set.body);
    expect_that!(signed_in.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn confirming_is_in_the_account_history() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;
    let token = token_of_mail(dir.path(), 1).await?;
    let confirmed = confirm(&app, &token).await?;
    let cookie = confirmed.session_cookie.context("no session cookie")?;

    // Act
    let history = app.send(Method::GET, "/api/v1/me/history", Some(&cookie), None).await?;

    // Assert
    let entries = history.body["entries"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<_> = entries.iter().filter_map(|entry| entry["kind"].as_str()).collect();
    let descriptions: Vec<_> = entries
        .iter()
        .filter_map(|entry| entry["description"].as_str())
        .collect();
    expect_that!(kinds, elements_are![eq(&"email_confirmed")]);
    expect_that!(
        descriptions,
        elements_are![eq(&"Address confirmed with the mailed link.")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_operator_confirming_is_in_the_account_history_and_signs_in_after() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;

    // Act
    app.services.confirm_email_by_operator("ada").await?;
    let signed_in = sign_in(&app, EMAIL, PASSWORD).await?;
    let cookie = signed_in.session_cookie.clone().context("no session cookie")?;
    let history = app.send(Method::GET, "/api/v1/me/history", Some(&cookie), None).await?;

    // Assert
    expect_that!(signed_in.status, eq(StatusCode::OK), "{}", signed_in.body);
    expect_that!(
        history.body["entries"][0]["description"],
        eq(&json!("Address confirmed by whoever runs the server."))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn without_mail_registering_signs_in_and_an_unconfirmed_account_signs_in() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let plain = TestApp::start().await?;
    register(&plain.with_mail(dir.path())?, EMAIL).await?;

    // Act
    let signed_in = sign_in(&plain, EMAIL, PASSWORD).await?;
    let registered = register(&plain, "grace@example.test").await?;

    // Assert
    expect_that!(signed_in.status, eq(StatusCode::OK));
    expect_that!(registered.status, eq(StatusCode::CREATED));
    Ok(())
}

async fn change_address(app: &TestApp, login: &str, password: &str, email: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/confirmation-address",
        None,
        Some(json!({ "login": login, "password": password, "email": email })),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn an_unconfirmed_account_changes_its_address_and_confirms_the_new_link() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, "ada@exmaple.test").await?;
    let first = token_of_mail(dir.path(), 1).await?;

    // Act
    let changed = change_address(&app, "ada@exmaple.test", PASSWORD, "  Ada@Example.test ").await?;
    let mails = common::wait_for_mails(dir.path(), 2).await?;
    let second = token_in(&mails[1])?;
    let old = confirm(&app, &first).await?;
    let confirmed = confirm(&app, &second).await?;
    let cookie = confirmed.session_cookie.clone().context("no session cookie")?;
    let signed_in = sign_in(&app, EMAIL, PASSWORD).await?;
    let history = app.send(Method::GET, "/api/v1/me/history", Some(&cookie), None).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::ACCEPTED), "{}", changed.body);
    expect_that!(changed.body, eq(&json!({ "email": EMAIL, "username": "ada" })));
    expect_that!(mails[1], contains_substring("To: ada@example.test"));
    expect_that!(problem_code(&old), eq("confirmation_link_cancelled"));
    expect_that!(confirmed.status, eq(StatusCode::OK), "{}", confirmed.body);
    expect_that!(signed_in.status, eq(StatusCode::OK));
    let entries = history.body["entries"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<_> = entries.iter().filter_map(|entry| entry["kind"].as_str()).collect();
    expect_that!(kinds, elements_are![eq(&"email_confirmed"), eq(&"email_changed")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_address_answers_the_same_and_changes_nothing() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, "ada@exmaple.test").await?;
    let first = token_of_mail(dir.path(), 1).await?;
    let other = register(&app, "grace@example.test").await?;
    common::wait_for_mails(dir.path(), 2).await?;

    // Act
    let changed = change_address(&app, "ada", PASSWORD, "grace@example.test").await?;
    let mailed = std::fs::read_dir(dir.path())?.count();
    let confirmed = confirm(&app, &first).await?;
    let signed_in = sign_in(&app, "ada@exmaple.test", PASSWORD).await?;

    // Assert
    expect_that!(other.status, eq(StatusCode::ACCEPTED));
    expect_that!(changed.status, eq(StatusCode::ACCEPTED), "{}", changed.body);
    expect_that!(
        changed.body,
        eq(&json!({ "email": "grace@example.test", "username": "ada" }))
    );
    expect_that!(mailed, eq(2));
    // The earlier link wasn't cancelled, and the account still has the address it was made with.
    expect_that!(confirmed.status, eq(StatusCode::OK), "{}", confirmed.body);
    expect_that!(signed_in.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn changing_the_address_with_a_wrong_password_is_refused_and_mails_nothing() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, "ada@exmaple.test").await?;
    common::wait_for_mails(dir.path(), 1).await?;

    // Act
    let changed = change_address(&app, "ada", "not the password", EMAIL).await?;
    let refused = sign_in(&app, "ada@exmaple.test", PASSWORD).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&changed), eq("invalid_credentials"));
    expect_that!(problem_code(&refused), eq("email_not_confirmed"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn changing_the_address_is_limited_like_signing_in() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, "ada@exmaple.test").await?;

    // Act
    let mut statuses = Vec::new();
    for _ in 0..5 {
        statuses.push(change_address(&app, "ada", "not the password", EMAIL).await?.status);
    }
    let limited = change_address(&app, "ada", PASSWORD, EMAIL).await?;

    // Assert
    expect_that!(statuses, each(eq(&StatusCode::UNAUTHORIZED)));
    expect_that!(limited.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(problem_code(&limited), eq("too_many_attempts"));
    expect_that!(limited.headers.contains_key("retry-after"), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_confirmed_account_cant_change_its_address_this_way() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, EMAIL).await?;
    app.services.confirm_email_by_operator("ada").await?;

    // Act
    let changed = change_address(&app, "ada", PASSWORD, "grace@example.test").await?;
    let signed_in = sign_in(&app, EMAIL, PASSWORD).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&changed), eq("invalid_credentials"));
    expect_that!(signed_in.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn without_mail_changing_the_address_is_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    register(&app, EMAIL).await?;

    // Act
    let changed = change_address(&app, "ada", PASSWORD, "grace@example.test").await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn changing_the_address_cancels_a_working_reset_link() -> Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let app = TestApp::start().await?.with_mail(dir.path())?;
    register(&app, "ada@exmaple.test").await?;
    let reset = app.make_reset_link("ada").await?;

    // Act
    change_address(&app, "ada", PASSWORD, EMAIL).await?;
    let used = app
        .send(
            Method::POST,
            &format!("/api/v1/password-resets/{reset}"),
            None,
            Some(json!({ "password": "another long password" })),
        )
        .await?;

    // Assert
    expect_that!(used.status, eq(StatusCode::CONFLICT));
    expect_that!(problem_code(&used), eq("reset_link_cancelled"));
    Ok(())
}
