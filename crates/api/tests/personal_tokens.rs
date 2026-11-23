//! Personal tokens: making, listing and revoking them from a session, and using one as a bearer token.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode, header};
use common::TestApp;
use domain::Duration;
use googletest::prelude::*;
use serde_json::json;

const EMAIL: &str = "ada@example.test";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn problem_code(response: &common::TestResponse) -> &str {
    response.body["code"].as_str().unwrap_or_default()
}

fn secret_of(response: &common::TestResponse) -> Result<String> {
    Ok(response.body["secret"].as_str().context("no secret")?.to_owned())
}

fn id_of(response: &common::TestResponse) -> Result<String> {
    Ok(response.body["token"]["id"].as_str().context("no id")?.to_owned())
}

async fn list(app: &TestApp, session: &str) -> Result<common::TestResponse> {
    app.send(Method::GET, "/api/v1/me/personal-tokens", Some(session), None)
        .await
}

async fn revoke(app: &TestApp, session: &str, id: &str) -> Result<common::TestResponse> {
    app.send(
        Method::DELETE,
        &format!("/api/v1/me/personal-tokens/{id}"),
        Some(session),
        None,
    )
    .await
}

#[gtest]
#[tokio::test]
async fn a_made_token_is_listed_without_its_secret_and_expires_in_ninety_days() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let made = app.make_personal_token(&session, "  ci ", None).await?;
    let never = app.make_personal_token(&session, "laptop", Some("never")).await?;
    let listed = list(&app, &session).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(secret_of(&made)?, starts_with("uph_"));
    expect_that!(made.body["token"]["name"], eq(&json!("ci")));
    let created = made.body["token"]["created_at"].as_i64().context("no created_at")?;
    expect_that!(made.body["token"]["expires_at"], eq(&json!(created + 90 * DAY_MS)));
    expect_that!(never.status, eq(StatusCode::CREATED));
    expect_that!(never.body["token"]["expires_at"], eq(&json!(null)));
    expect_that!(never.body["token"]["expires_label"], eq(&json!(null)));
    expect_that!(listed.status, eq(StatusCode::OK));
    let tokens = listed.body["tokens"].as_array().context("no tokens")?;
    expect_that!(tokens.len(), eq(2));
    expect_that!(tokens[0]["name"], eq(&json!("laptop")));
    expect_that!(tokens[1]["name"], eq(&json!("ci")));
    expect_that!(tokens[1]["last_used_at"], eq(&json!(null)));
    expect_that!(tokens[1]["last_used_label"], eq(&json!(null)));
    expect_that!(tokens[1]["created_label"], eq(&json!("today")));
    expect_that!(tokens[1].get("secret"), none());
    expect_that!(
        listed.body.to_string(),
        not(contains_substring(secret_of(&made)?.as_str()))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_secret_works_as_a_bearer_token_and_records_its_use() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let secret = secret_of(&app.make_personal_token(&session, "ci", None).await?)?;

    // Act
    let me = app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;
    let listed = list(&app, &session).await?;

    // Assert
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(me.body["user"]["email"], eq(&json!(EMAIL)));
    expect_that!(listed.body["tokens"][0]["last_used_at"], not(eq(&json!(null))));
    expect_that!(listed.body["tokens"][0]["last_used_label"], eq(&json!("today")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn last_used_is_written_at_most_once_a_minute() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let secret = secret_of(&app.make_personal_token(&session, "ci", None).await?)?;
    app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;
    let first = list(&app, &session).await?.body["tokens"][0]["last_used_at"].clone();

    // Act
    app.clock.advance(Duration::from_millis(30_000));
    app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;
    let within = list(&app, &session).await?.body["tokens"][0]["last_used_at"].clone();
    app.clock.advance(Duration::from_millis(31_000));
    app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;
    let after = list(&app, &session).await?.body["tokens"][0]["last_used_at"].clone();

    // Assert
    expect_that!(within, eq(&first));
    expect_that!(after.as_i64(), some(eq(first.as_i64().context("never used")? + 61_000)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_revoked_token_is_refused_and_gone_from_the_list() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let made = app.make_personal_token(&session, "ci", None).await?;
    let secret = secret_of(&made)?;
    let id = id_of(&made)?;

    // Act
    let revoked = revoke(&app, &session, &id).await?;
    let used = app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;
    let again = revoke(&app, &session, &id).await?;
    let listed = list(&app, &session).await?;

    // Assert
    expect_that!(revoked.status, eq(StatusCode::NO_CONTENT));
    expect_that!(used.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&used), eq("token_revoked"));
    expect_that!(used.body["title"], eq(&json!("The token was revoked")));
    expect_that!(
        used.headers.get(header::WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()),
        some(eq("Bearer error=\"invalid_token\""))
    );
    expect_that!(again.status, eq(StatusCode::NO_CONTENT));
    expect_that!(listed.body["tokens"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_expired_token_is_refused_and_not_listed() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let made = app.make_personal_token(&session, "ci", Some("30_days")).await?;
    let secret = secret_of(&made)?;
    let id = id_of(&made)?;

    // Act
    app.clock.advance(Duration::from_days(31));
    let session = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "login": EMAIL, "password": "a long test password" })),
        )
        .await?
        .session_cookie
        .context("sign-in set no cookie")?;
    let used = app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;
    let listed = list(&app, &session).await?;
    let revoked = revoke(&app, &session, &id).await?;

    // Assert
    expect_that!(used.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&used), eq("token_expired"));
    expect_that!(
        used.headers.get(header::WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()),
        some(eq("Bearer error=\"invalid_token\""))
    );
    expect_that!(listed.body["tokens"], eq(&json!([])));
    expect_that!(revoked.status, eq(StatusCode::NO_CONTENT));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_token_is_unauthenticated() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let used = app
        .send_bearer(Method::GET, "/api/v1/me", "uph_notatokenthisserverknows", None)
        .await?;

    // Assert
    expect_that!(used.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(problem_code(&used), eq("unauthenticated"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn another_users_token_cannot_be_revoked_and_is_not_told_apart_from_none() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let (other, _) = app.register("grace@example.test").await?;
    let made = app.make_personal_token(&session, "ci", None).await?;
    let secret = secret_of(&made)?;

    // Act
    let revoked = revoke(&app, &other, &id_of(&made)?).await?;
    let unknown = revoke(&app, &other, "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40").await?;
    let used = app.send_bearer(Method::GET, "/api/v1/me", &secret, None).await?;

    // Assert
    expect_that!(revoked.status, eq(StatusCode::NOT_FOUND));
    expect_that!(unknown.status, eq(StatusCode::NOT_FOUND));
    expect_that!(used.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bearer_token_cannot_make_or_list_tokens() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;
    let secret = secret_of(&app.make_personal_token(&session, "ci", None).await?)?;

    // Act
    let made = app
        .send_bearer(
            Method::POST,
            "/api/v1/me/personal-tokens",
            &secret,
            Some(json!({ "name": "more" })),
        )
        .await?;
    let listed = app
        .send_bearer(Method::GET, "/api/v1/me/personal-tokens", &secret, None)
        .await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(listed.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(
        list(&app, &session).await?.body["tokens"].as_array().map(Vec::len),
        some(eq(1))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bad_name_or_expiry_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let empty = app.make_personal_token(&session, "   ", None).await?;
    let too_long = app.make_personal_token(&session, &"x".repeat(65), None).await?;
    let control = app.make_personal_token(&session, "a\nb", None).await?;
    let expiry = app.make_personal_token(&session, "ci", Some("2_days")).await?;

    // Assert
    expect_that!(empty.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(empty.body["field"], eq(&json!("name")));
    expect_that!(too_long.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(too_long.body["field"], eq(&json!("name")));
    expect_that!(control.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(expiry.status.is_client_error(), eq(true));
    expect_that!(list(&app, &session).await?.body["tokens"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_field_the_token_request_does_not_have_is_refused_and_no_token_is_made() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register(EMAIL).await?;

    // Act
    let made = app
        .send(
            Method::POST,
            "/api/v1/me/personal-tokens",
            Some(&session),
            Some(json!({ "name": "laptop", "expires": "never" })),
        )
        .await?;
    let listed = list(&app, &session).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(listed.body["tokens"].as_array().map(Vec::len), some(eq(0)));
    Ok(())
}
