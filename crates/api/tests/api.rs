//! End-to-end API behaviour over the real router and an in-memory database.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::Duration;
use domain::team::Role;
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
    expect_that!(
        me.body["allowed_actions"],
        eq(&json!([
            "sign_out",
            "create_team",
            "manage_sign_ins",
            "change_password"
        ]))
    );
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
    expect_that!(response.body["field"], eq(&json!("email")));
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
        Some(json!({ "need": "expected", "title": "Large", "effort": "xl", "impact": "l" })),
    )
    .await?;
    app.send(
        Method::POST,
        &items,
        Some(&session),
        Some(json!({ "need": "expected", "title": "Small", "effort": "s", "impact": "l" })),
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
async fn items_say_who_added_them_and_how_long_ago() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register_as("ada@example.test", "Ada").await?;
    let created = app
        .new_item(&session, &heap, json!({ "title": "Write the docs" }))
        .await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    app.clock.advance(Duration::from_days(4));

    // Act
    let item = app
        .send(
            Method::GET,
            &app.item_url(created["id"].as_str().unwrap_or_default()).await?,
            Some(&session),
            None,
        )
        .await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(created["created_label"], eq(&json!("today")));
    expect_that!(created["created_by"]["display_name"], eq(&json!("Ada")));
    expect_that!(created["created_by"]["id"], eq(&me.body["user"]["id"]));
    expect_that!(item.body["created_label"], eq(&json!("4 days ago")));
    expect_that!(item.body["created_by"]["display_name"], eq(&json!("Ada")));
    let listed = &list.body["ranked"][0]["item"];
    expect_that!(listed["created_label"], eq(&json!("4 days ago")));
    expect_that!(listed["created_by"]["display_name"], eq(&json!("Ada")));
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
            Some(json!({ "need": "expected", "title": "Task" })),
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
                Some(json!({ "need": "expected", "title": title })),
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
    expect_that!(closed.body["items"][0]["allowed_actions"], eq(&json!(["comment"])));
    Ok(())
}

/// Closes the item `created`, as returned by the API, as the user of `session`, and returns the closed item.
async fn close_item(
    app: &TestApp,
    session: &str,
    created: &serde_json::Value,
    state: &str,
) -> Result<serde_json::Value> {
    let url = app.item_url(created["id"].as_str().unwrap_or_default()).await?;
    let closed = app
        .patch(session, &url, &created["version"], json!({ "state": state }))
        .await?;
    anyhow::ensure!(closed.status == StatusCode::OK, "close failed: {}", closed.body);
    Ok(closed.body)
}

#[gtest]
#[tokio::test]
async fn closed_items_say_who_closed_them_and_are_grouped_by_day() -> Result<()> {
    // Arrange: Ada closes "Old"; three days later Sam, another member, closes "Mid"; four days after that Ada
    // closes "New".
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let (sam, _) = app.register_as("sam@example.test", "Sam").await?;
    let sam_me = app.send(Method::GET, "/api/v1/me", Some(&sam), None).await?;
    let (team, heap) = app.shared_heap(&ada).await?;
    app.add_member(
        &team,
        sam_me.body["user"]["id"].as_str().unwrap_or_default(),
        Role::Member,
    )
    .await?;
    let mut created = Vec::new();
    for title in ["Old", "Mid", "New"] {
        created.push(app.new_item(&ada, &heap, json!({ "title": title })).await?);
    }
    close_item(&app, &ada, &created[0], "done").await?;
    app.clock.advance(Duration::from_days(3));
    close_item(&app, &sam, &created[1], "wont_do").await?;
    app.clock.advance(Duration::from_days(4));
    let new = close_item(&app, &ada, &created[2], "done").await?;

    // Act
    let closed = app
        .send(
            Method::GET,
            &format!("{}/items/closed", app.heap_url(&heap).await?),
            Some(&ada),
            None,
        )
        .await?;
    let mid = app
        .send(
            Method::GET,
            &app.item_url(created[1]["id"].as_str().unwrap_or_default()).await?,
            Some(&ada),
            None,
        )
        .await?;

    // Assert
    expect_that!(new["closed_by"]["display_name"], eq(&json!("Ada")));
    expect_that!(new["closed_group"], eq(&json!("today")));
    let items = &closed.body["items"];
    let summary = |i: usize| {
        (
            items[i]["title"].clone(),
            items[i]["closed_group"].clone(),
            items[i]["closed_by"]["display_name"].clone(),
        )
    };
    expect_that!(summary(0), eq(&(json!("New"), json!("today"), json!("Ada"))));
    expect_that!(summary(1), eq(&(json!("Mid"), json!("this_week"), json!("Sam"))));
    expect_that!(summary(2), eq(&(json!("Old"), json!("earlier"), json!("Ada"))));
    expect_that!(items[1]["closed_by"]["id"], eq(&sam_me.body["user"]["id"]));
    expect_that!(mid.body["closed_by"]["display_name"], eq(&json!("Sam")));
    expect_that!(mid.body["closed_group"], eq(&json!("this_week")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reopening_clears_who_closed_an_item() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register_as("ada@example.test", "Ada").await?;
    let created = app.new_item(&session, &heap, json!({ "title": "Ship it" })).await?;
    let closed = close_item(&app, &session, &created, "done").await?;
    let url = app.item_url(created["id"].as_str().unwrap_or_default()).await?;

    // Act
    let reopened = app
        .send_with(
            Method::POST,
            &format!("{url}/reopen"),
            Some(&session),
            None,
            &[common::CSRF, ("if-match", &closed["version"].to_string())],
        )
        .await?;
    let item = app.send(Method::GET, &url, Some(&session), None).await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(reopened.status, eq(StatusCode::OK));
    for body in [&reopened.body, &item.body, &list.body["ranked"][0]["item"]] {
        expect_that!(body["closed_by"], eq(&json!(null)));
        expect_that!(body["closed_group"], eq(&json!(null)));
        expect_that!(body["closed_at"], eq(&json!(null)));
    }
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
            Some(json!({ "need": "expected", "title": "x" })),
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

async fn sign_in(app: &TestApp, login: &str, password: &str) -> Result<common::TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({ "login": login, "password": password })),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn users_sign_in_by_username_or_by_email() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;

    // Act
    let by_username = sign_in(&app, "ada", "a long test password").await?;
    let capitalised = sign_in(&app, " Ada ", "a long test password").await?;
    let wrong_password = sign_in(&app, "ada", "not the password").await?;
    let unknown = sign_in(&app, "nobody", "a long test password").await?;
    let not_a_username = sign_in(&app, "ada lovelace", "a long test password").await?;
    // Clients written before usernames send `email`, which still works.
    let old_field = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "ada@example.test", "password": "a long test password" })),
        )
        .await?;

    // Assert
    expect_that!(by_username.status, eq(StatusCode::OK));
    expect_that!(by_username.body["username"], eq(&json!("ada")));
    expect_that!(by_username.session_cookie, some(anything()));
    expect_that!(capitalised.status, eq(StatusCode::OK));
    expect_that!(wrong_password.body["code"], eq(&json!("invalid_credentials")));
    expect_that!(unknown.body["code"], eq(&json!("invalid_credentials")));
    expect_that!(not_a_username.body["code"], eq(&json!("invalid_credentials")));
    expect_that!(old_field.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_account_has_one_count_of_attempts_by_username_and_email_together() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;
    for _ in 0..3 {
        sign_in(&app, "ada", "not the password").await?;
    }
    for _ in 0..2 {
        sign_in(&app, "ada@example.test", "not the password").await?;
    }
    for _ in 0..5 {
        sign_in(&app, "nobody", "whatever it is").await?;
    }

    // Act
    let by_username = sign_in(&app, "ada", "a long test password").await?;
    let by_email = sign_in(&app, "ada@example.test", "a long test password").await?;
    let unknown = sign_in(&app, "nobody", "whatever it is").await?;

    // Assert
    expect_that!(by_username.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(by_email.status, eq(StatusCode::TOO_MANY_REQUESTS));
    expect_that!(unknown.body["code"], eq(&json!("too_many_attempts")));
    Ok(())
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
async fn empty_sign_in_fields_are_rejected_without_counting_an_attempt() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    app.register("ada@example.test").await?;

    // Act
    let mut empty = Vec::new();
    for _ in 0..7 {
        empty.push(sign_in(&app, "ada@example.test", "").await?);
        empty.push(sign_in(&app, " ", "a long test password").await?);
    }
    let real = sign_in(&app, "ada@example.test", "a long test password").await?;

    // Assert
    let statuses: Vec<_> = empty.iter().map(|response| response.status).collect();
    let codes: Vec<_> = empty.iter().map(|response| response.body["code"].clone()).collect();
    let fields: Vec<_> = empty.iter().map(|response| response.body["field"].clone()).collect();
    expect_that!(statuses, each(eq(&StatusCode::UNPROCESSABLE_ENTITY)));
    expect_that!(codes, each(eq(&json!("field_empty"))));
    expect_that!(fields.first(), some(eq(&json!("password"))));
    expect_that!(fields.get(1), some(eq(&json!("login"))));
    expect_that!(real.status, eq(StatusCode::OK));
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
