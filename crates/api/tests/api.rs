//! End-to-end API behaviour over the real router and an in-memory database.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::Duration;
use googletest::prelude::*;
use serde_json::json;

#[gtest]
#[tokio::test]
async fn health_probes_answer_without_a_session() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let live = app.send(Method::GET, "/healthz", None, None).await?;
    let ready = app.send(Method::GET, "/readyz", None, None).await?;

    // Assert
    expect_that!(live.status, eq(StatusCode::OK));
    expect_that!(ready.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn registering_creates_a_personal_heap_and_signs_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let (session, _) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;

    // Assert
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(me.body["teams"][0]["role"], eq(&json!("admin")));
    expect_that!(me.body["teams"][0]["heaps"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(me.body["allowed_actions"], eq(&json!(["sign_out", "create_team"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn duplicate_email_is_rejected() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;
    let again = json!({ "email": "ADA@example.test", "username": "ada2", "display_name": "Ada", "password": "a long test password" });

    // Act
    let response = app
        .send(Method::POST, "/api/v1/auth/register", None, Some(again))
        .await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::CONFLICT));
    expect_that!(response.body["code"], eq(&json!("email_taken")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_are_ranked_with_contributors() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let items = format!("{}/items", app.heap_url(&heap).await?);
    app.send(
        Method::POST,
        &items,
        Some(&session),
        Some(json!({ "title": "Large", "effort": "xl", "impact": "l" })),
    )
    .await?;
    app.send(
        Method::POST,
        &items,
        Some(&session),
        Some(json!({ "title": "Small", "effort": "s", "impact": "l" })),
    )
    .await?;

    // Act
    let list = app.send(Method::GET, &items, Some(&session), None).await?;

    // Assert
    expect_that!(list.status, eq(StatusCode::OK));
    expect_that!(list.body["ranked"][0]["item"]["title"], eq(&json!("Small")));
    expect_that!(list.body["ranked"][1]["item"]["title"], eq(&json!("Large")));
    expect_that!(
        list.body["ranked"][0]["contributors"][0]["label"],
        eq(&json!("L impact, S effort"))
    );
    expect_that!(list.body["as_of"].as_i64(), some(eq(common::START)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn edits_need_the_current_version() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let created = app
        .send(
            Method::POST,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            Some(json!({ "title": "Task" })),
        )
        .await?;
    let item = app.item_url(created.body["id"].as_str().unwrap_or_default()).await?;
    let rename = json!({ "title": "Renamed" });
    let csrf = ("x-csrf-protection", "1");

    // Act
    let missing = app
        .send(Method::PATCH, &item, Some(&session), Some(rename.clone()))
        .await?;
    let first = app
        .send_with(
            Method::PATCH,
            &item,
            Some(&session),
            Some(rename.clone()),
            &[csrf, ("if-match", "\"1\"")],
        )
        .await?;
    let stale = app
        .send_with(
            Method::PATCH,
            &item,
            Some(&session),
            Some(json!({ "title": "Other" })),
            &[csrf, ("if-match", "\"1\"")],
        )
        .await?;

    // Assert
    expect_that!(missing.status, eq(StatusCode::PRECONDITION_REQUIRED));
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(first.body["version"], eq(&json!(2)));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn closed_items_reopen_only_within_a_week() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let csrf = ("x-csrf-protection", "1");
    let mut ids = Vec::new();
    for title in ["Early", "Late"] {
        let created = app
            .send(
                Method::POST,
                &format!("{}/items", app.heap_url(&heap).await?),
                Some(&session),
                Some(json!({ "title": title })),
            )
            .await?;
        let id = created.body["id"].as_str().unwrap_or_default().to_owned();
        app.send_with(
            Method::PATCH,
            &app.item_url(&id).await?,
            Some(&session),
            Some(json!({ "state": "done" })),
            &[csrf, ("if-match", "1")],
        )
        .await?;
        ids.push(id);
    }

    // Act
    app.clock.advance(Duration::from_days(6));
    let inside = app
        .send_with(
            Method::POST,
            &format!("{}/reopen", app.item_url(&ids[0]).await?),
            Some(&session),
            None,
            &[csrf, ("if-match", "2")],
        )
        .await?;
    app.clock.advance(Duration::from_days(2));
    let outside = app
        .send_with(
            Method::POST,
            &format!("{}/reopen", app.item_url(&ids[1]).await?),
            Some(&session),
            None,
            &[csrf, ("if-match", "2")],
        )
        .await?;
    let closed = app
        .send(
            Method::GET,
            &format!("{}/items/closed", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(inside.status, eq(StatusCode::OK));
    expect_that!(inside.body["state"], eq(&json!("open")));
    expect_that!(outside.status, eq(StatusCode::CONFLICT));
    expect_that!(outside.body["code"], eq(&json!("reopen_window_expired")));
    expect_that!(closed.body["items"][0]["allowed_actions"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn outsiders_get_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (_, heap) = app.register("ada@example.test").await?;
    let (outsider, _) = app.register("eve@example.test").await?;

    // Act
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&outsider),
            None,
        )
        .await?;
    let create = app
        .send(
            Method::POST,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&outsider),
            Some(json!({ "title": "x" })),
        )
        .await?;

    // Assert
    expect_that!(list.status, eq(StatusCode::NOT_FOUND));
    expect_that!(create.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn writes_without_the_csrf_header_are_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let body = json!({ "email": "ada@example.test", "username": "ada", "display_name": "Ada", "password": "a long test password" });

    // Act
    let response = app
        .send_with(Method::POST, "/api/v1/auth/register", None, Some(body), &[])
        .await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::FORBIDDEN));
    expect_that!(response.body["code"], eq(&json!("csrf_header_missing")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sessions_expire_and_sign_out_ends_them() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let signed_in = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "ada@example.test", "password": "a long test password" })),
        )
        .await?;
    let second = signed_in.session_cookie.unwrap_or_default();

    // Act
    app.send(Method::POST, "/api/v1/auth/logout", Some(&session), None)
        .await?;
    let after_logout = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    app.clock.advance(Duration::from_days(31));
    let after_expiry = app.send(Method::GET, "/api/v1/me", Some(&second), None).await?;

    // Assert
    expect_that!(signed_in.status, eq(StatusCode::OK));
    expect_that!(after_logout.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(after_expiry.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

async fn sign_in(app: &TestApp, email: &str, password: &str) -> Result<common::TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn sign_in_attempts_are_limited_per_address_until_the_window_ends() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;
    for _ in 0..5 {
        sign_in(&app, "Ada@Example.test", "not the password").await?;
        sign_in(&app, "nobody@example.test", "whatever it is").await?;
    }
    app.clock.advance(Duration::from_minutes(5));

    // Act
    let refused = sign_in(&app, "ada@example.test", "a long test password").await?;
    let unknown = sign_in(&app, "nobody@example.test", "whatever it is").await?;
    app.register("bob@example.test").await?;
    let other = sign_in(&app, "bob@example.test", "a long test password").await?;
    app.clock.advance(Duration::from_minutes(10));
    let after_window = sign_in(&app, "ada@example.test", "a long test password").await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(refused.body["code"], eq(&json!("too_many_attempts")));
    expect_that!(
        refused.body["title"],
        eq(&json!("too many sign-in attempts; try again in 10 minutes"))
    );
    expect_that!(refused.retry_after.as_deref(), some(eq("600")));
    expect_that!(refused.session_cookie, none());
    expect_that!(unknown.body["code"], eq(&json!("too_many_attempts")));
    expect_that!(other.status, eq(StatusCode::OK));
    expect_that!(after_window.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn signing_in_starts_the_count_over() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;
    for _ in 0..4 {
        sign_in(&app, "ada@example.test", "not the password").await?;
    }
    sign_in(&app, "ada@example.test", "a long test password").await?;

    // Act
    let mut statuses = Vec::new();
    for _ in 0..5 {
        statuses.push(sign_in(&app, "ada@example.test", "not the password").await?.status);
    }
    let sixth = sign_in(&app, "ada@example.test", "not the password").await?;

    // Assert
    expect_that!(statuses, each(eq(&StatusCode::UNAUTHORIZED)));
    expect_that!(sixth.status, eq(StatusCode::TOO_MANY_REQUESTS));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn malformed_addresses_are_not_counted() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let mut codes = Vec::new();
    for _ in 0..7 {
        codes.push(sign_in(&app, "not an address", "whatever it is").await?.body["code"].clone());
    }

    // Assert
    expect_that!(codes, each(eq(&json!("invalid_credentials"))));
    Ok(())
}

/// On MySQL, the default collation would find Ada under these spellings, each with attempts of its own.
#[gtest]
#[tokio::test]
async fn an_account_is_found_only_under_its_own_address() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;

    // Act
    let accented = sign_in(&app, "ad\u{e1}@example.test", "a long test password").await?;
    let zero_width = sign_in(&app, "ada\u{200b}@example.test", "a long test password").await?;
    let fullwidth = sign_in(&app, "\u{ff41}da@example.test", "a long test password").await?;
    let own = sign_in(&app, "ADA@example.test", "a long test password").await?;

    // Assert
    expect_that!(accented.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(zero_width.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(fullwidth.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(own.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn wrong_password_is_rejected() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;

    // Act
    let wrong = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "ada@example.test", "password": "not the password" })),
        )
        .await?;
    let unknown = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "nobody@example.test", "password": "whatever it is" })),
        )
        .await?;

    // Assert
    expect_that!(wrong.body["code"], eq(&json!("invalid_credentials")));
    expect_that!(unknown.body["code"], eq(&json!("invalid_credentials")));
    Ok(())
}
