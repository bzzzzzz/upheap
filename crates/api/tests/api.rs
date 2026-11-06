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
    Ok(())
}

#[gtest]
#[tokio::test]
async fn duplicate_email_is_rejected() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;
    let again = json!({ "email": "ADA@example.test", "display_name": "Ada", "password": "a long test password" });

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
    let items = format!("/api/v1/heaps/{heap}/items");
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
            &format!("/api/v1/heaps/{heap}/items"),
            Some(&session),
            Some(json!({ "title": "Task" })),
        )
        .await?;
    let item = format!("/api/v1/items/{}", created.body["id"].as_str().unwrap_or_default());
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
                &format!("/api/v1/heaps/{heap}/items"),
                Some(&session),
                Some(json!({ "title": title })),
            )
            .await?;
        let id = created.body["id"].as_str().unwrap_or_default().to_owned();
        app.send_with(
            Method::PATCH,
            &format!("/api/v1/items/{id}"),
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
            &format!("/api/v1/items/{}/reopen", ids[0]),
            Some(&session),
            None,
            &[csrf, ("if-match", "2")],
        )
        .await?;
    app.clock.advance(Duration::from_days(2));
    let outside = app
        .send_with(
            Method::POST,
            &format!("/api/v1/items/{}/reopen", ids[1]),
            Some(&session),
            None,
            &[csrf, ("if-match", "2")],
        )
        .await?;
    let closed = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/items/closed"),
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
            &format!("/api/v1/heaps/{heap}/items"),
            Some(&outsider),
            None,
        )
        .await?;
    let create = app
        .send(
            Method::POST,
            &format!("/api/v1/heaps/{heap}/items"),
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
    let body = json!({ "email": "ada@example.test", "display_name": "Ada", "password": "a long test password" });

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
