//! The operator role and its endpoints: who may call them, the account list, turning a local account off and on,
//! reset links and the history as an operator reads it.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

const PASSWORD: &str = "a long test password";

async fn get(app: &TestApp, session: &str, path: &str) -> Result<TestResponse> {
    app.send(Method::GET, &format!("/api/v1{path}"), Some(session), None)
        .await
}

async fn post(app: &TestApp, session: &str, path: &str) -> Result<TestResponse> {
    app.send(Method::POST, &format!("/api/v1{path}"), Some(session), None)
        .await
}

async fn user_id(app: &TestApp, session: &str) -> Result<String> {
    let me = get(app, session, "/me").await?;
    Ok(me.body["user"]["id"].as_str().context("no user id")?.to_owned())
}

async fn login(app: &TestApp, email: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({ "login": email, "password": PASSWORD })),
    )
    .await
}

fn descriptions(history: &TestResponse) -> Vec<&str> {
    history.body["entries"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| entry["description"].as_str())
        .collect()
}

#[gtest]
#[tokio::test]
async fn a_caller_who_is_no_operator_gets_not_found_from_every_operator_endpoint() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let id = user_id(&app, &session).await?;
    let base = format!("/operator/accounts/{id}");

    // Act
    let me = get(&app, &session, "/me").await?;
    let responses = [
        get(&app, &session, "/operator/accounts").await?,
        get(&app, &session, &format!("{base}/sign-ins")).await?,
        post(&app, &session, &format!("{base}/deactivate")).await?,
        post(&app, &session, &format!("{base}/reactivate")).await?,
        post(&app, &session, &format!("{base}/reset-link")).await?,
        get(&app, &session, &format!("{base}/history")).await?,
    ];
    let anonymous = app.send(Method::GET, "/api/v1/operator/accounts", None, None).await?;

    // Assert
    expect_that!(me.body["operator"], eq(&json!(false)));
    for response in &responses {
        expect_that!(response.status, eq(StatusCode::NOT_FOUND));
        expect_that!(response.body["code"], eq(&json!("not_found")));
    }
    expect_that!(anonymous.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_operator_lists_every_account_and_losing_the_role_ends_it_at_once() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    let (_bo, _) = app.register_as("bo@example.test", "bo Park").await?;
    app.services.grant_operator("ada").await?;
    app.clock.advance(Duration::from_days(3));

    // Act
    let me = get(&app, &ada, "/me").await?;
    let listed = get(&app, &ada, "/operator/accounts").await?;
    app.services.remove_operator("ada").await?;
    let after = get(&app, &ada, "/operator/accounts").await?;

    // Assert
    expect_that!(me.body["operator"], eq(&json!(true)));
    expect_that!(listed.status, eq(StatusCode::OK));
    let accounts = listed.body["accounts"].as_array().context("no accounts")?;
    expect_that!(
        accounts.iter().map(|a| a["username"].clone()).collect::<Vec<_>>(),
        eq(&vec![json!("ada"), json!("bo")])
    );
    expect_that!(accounts[0]["kind"], eq(&json!("local")));
    expect_that!(accounts[0]["operator"], eq(&json!(true)));
    expect_that!(accounts[0]["confirmed"], eq(&json!(true)));
    expect_that!(accounts[0]["deactivated"], eq(&json!(false)));
    expect_that!(accounts[0]["last_sign_in_label"], eq(&json!("3 days ago")));
    expect_that!(accounts[1]["operator"], eq(&json!(false)));
    expect_that!(after.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn turning_an_account_off_ends_every_sign_in_and_blocks_the_next_one() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    app.services.grant_operator("ada").await?;
    let (bo, _) = app.register_as("bo@example.test", "Bo Park").await?;
    let (access, _) = app.sign_in_cli(&bo).await?;
    let made = app.make_personal_token(&bo, "ci", None).await?;
    let personal = made.body["secret"].as_str().context("no secret")?.to_owned();
    let bo_id = user_id(&app, &bo).await?;
    let base = format!("/operator/accounts/{bo_id}");

    // Act
    let counts = get(&app, &admin, &format!("{base}/sign-ins")).await?;
    let off = post(&app, &admin, &format!("{base}/deactivate")).await?;
    let off_again = post(&app, &admin, &format!("{base}/deactivate")).await?;
    let session_after = get(&app, &bo, "/me").await?;
    let access_after = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;
    let personal_after = app.send_bearer(Method::GET, "/api/v1/me", &personal, None).await?;
    let refused = login(&app, "bo@example.test").await?;
    let listed = get(&app, &admin, "/operator/accounts").await?;
    let history = get(&app, &admin, &format!("{base}/history")).await?;

    // Assert
    expect_that!(counts.body, eq(&json!({ "sign_ins": 2, "personal_tokens": 1 })));
    expect_that!(off.status, eq(StatusCode::OK));
    expect_that!(off.body, eq(&json!({ "ended_sign_ins": 3 })));
    expect_that!(off_again.body, eq(&json!({ "ended_sign_ins": 0 })));
    expect_that!(session_after.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(access_after.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(personal_after.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(refused.body["code"], eq(&json!("account_turned_off")));
    expect_that!(listed.body["accounts"][1]["deactivated"], eq(&json!(true)));
    expect_that!(
        descriptions(&history),
        elements_are![eq(&"Deactivated by Ada Lee. 3 sign-ins ended.")]
    );
    let own = app
        .services
        .list_account_history(
            service::Principal {
                user_id: domain::UserId::from_uuid(bo_id.parse()?),
                run: None,
            },
            None,
            50,
            &jiff::tz::TimeZone::UTC,
        )
        .await?;
    let own: Vec<&str> = own.entries.iter().map(|e| e.description.as_str()).collect();
    expect_that!(
        own,
        elements_are![eq(&"Turned off by whoever runs the server. 3 sign-ins ended.")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn turning_an_account_back_on_lets_the_password_in_and_repeating_records_nothing() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    app.services.grant_operator("ada").await?;
    let (bo, _) = app.register_as("bo@example.test", "Bo Park").await?;
    let bo_id = user_id(&app, &bo).await?;
    let base = format!("/operator/accounts/{bo_id}");
    post(&app, &admin, &format!("{base}/deactivate")).await?;

    // Act
    let on = post(&app, &admin, &format!("{base}/reactivate")).await?;
    let on_again = post(&app, &admin, &format!("{base}/reactivate")).await?;
    let signed_in = login(&app, "bo@example.test").await?;
    let history = get(&app, &admin, &format!("{base}/history")).await?;

    // Assert
    expect_that!(on.status, eq(StatusCode::NO_CONTENT));
    expect_that!(on_again.status, eq(StatusCode::NO_CONTENT));
    expect_that!(signed_in.status, eq(StatusCode::OK));
    expect_that!(
        descriptions(&history),
        elements_are![
            eq(&"Reactivated by Ada Lee."),
            eq(&"Deactivated by Ada Lee. 1 sign-in ended."),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reset_link_is_made_for_the_account_by_the_operator_and_cancels_the_older_one() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    app.services.grant_operator("ada").await?;
    let (bo, _) = app.register_as("bo@example.test", "Bo Park").await?;
    let bo_id = user_id(&app, &bo).await?;
    let path = format!("/operator/accounts/{bo_id}/reset-link");
    let older = app.make_reset_link("bo").await?;

    // Act
    let made = post(&app, &admin, &path).await?;
    let older_opened = app
        .send(Method::GET, &format!("/api/v1/password-resets/{older}"), None, None)
        .await?;
    let history = get(&app, &admin, &format!("/operator/accounts/{bo_id}/history")).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::OK));
    let url = made.body["url"].as_str().context("no url")?;
    expect_that!(url, starts_with("https://upheap.example/reset/"));
    expect_that!(
        made.body["expires_at"],
        eq(&json!(common::START + Duration::from_hours(1).as_millis()))
    );
    expect_that!(older_opened.body["state"], not(eq(&json!("working"))));
    expect_that!(descriptions(&history)[0], eq("Reset link made by Ada Lee."));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn provider_accounts_are_listed_read_only() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    app.services.grant_operator("ada").await?;
    let (bo, _) = app.register_as("bo@example.test", "Bo Park").await?;
    let bo_id = user_id(&app, &bo).await?;
    let issuer = "https://issuer.example";
    app.link_identity(&bo_id, issuer, "subject-1").await?;
    let with_provider = app.with_oidc(common::oidc::settings_for(issuer)?);
    let base = format!("/operator/accounts/{bo_id}");

    // Act
    let listed = get(&with_provider, &admin, "/operator/accounts").await?;
    let off = post(&with_provider, &admin, &format!("{base}/deactivate")).await?;
    let on = post(&with_provider, &admin, &format!("{base}/reactivate")).await?;
    let history = get(&with_provider, &admin, &format!("{base}/history")).await?;

    // Assert
    expect_that!(listed.body["accounts"][1]["kind"], eq(&json!("provider")));
    expect_that!(off.status, eq(StatusCode::CONFLICT));
    expect_that!(off.body["code"], eq(&json!("account_provider_managed")));
    expect_that!(on.status, eq(StatusCode::CONFLICT));
    expect_that!(history.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reset_link_is_refused_while_only_the_provider_signs_people_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    app.services.grant_operator("ada").await?;
    let (bo, _) = app.register_as("bo@example.test", "Bo Park").await?;
    let bo_id = user_id(&app, &bo).await?;
    let provider_only = app.with_settings(service::Settings {
        password_sign_in: false,
        oidc: Some(common::oidc::settings_for("https://issuer.example")?),
        ..service::Settings::default()
    });

    // Act: the operator is a local account too, so the provider-only server refuses them first; read the answer
    // through the service instead.
    let refused = provider_only
        .services
        .create_operator_reset_link(
            service::Principal {
                user_id: domain::UserId::from_uuid(user_id(&app, &admin).await?.parse()?),
                run: None,
            },
            domain::UserId::from_uuid(bo_id.parse()?),
        )
        .await;

    // Assert
    expect_that!(
        refused,
        err(matches_pattern!(service::ServiceError::PasswordSignInOff { .. }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn signing_in_sets_the_last_sign_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada Lee").await?;
    app.services.grant_operator("ada").await?;
    app.clock.advance(Duration::from_days(20));

    // Act
    let before = get(&app, &admin, "/operator/accounts").await?;
    login(&app, "ada@example.test").await?;
    let after = get(&app, &admin, "/operator/accounts").await?;

    // Assert
    expect_that!(
        before.body["accounts"][0]["last_sign_in_label"],
        eq(&json!("2 weeks ago"))
    );
    expect_that!(
        after.body["accounts"][0]["last_sign_in_label"],
        eq(&Value::from("today"))
    );
    Ok(())
}
