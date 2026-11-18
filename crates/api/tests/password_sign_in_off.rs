//! A server that signs in through its provider only (`password_sign_in` off): which password paths are refused, how
//! a local user's sign-ins end, and that provider users carry on.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode, header};
use common::oidc::{self, APP_URL, Claims, FakeProvider, LABEL};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};
use service::Settings;

const PASSWORD: &str = "a long test password";

/// The server with passwords on and no provider, and the same server with passwords off.
struct Harness {
    provider: FakeProvider,
    on: TestApp,
    off: TestApp,
}

async fn harness() -> Result<Harness> {
    let provider = FakeProvider::start().await?;
    let on = TestApp::start().await?;
    let off = off_app(&on, &provider)?;
    Ok(Harness { provider, on, off })
}

/// A second router over the same database, with the provider and passwords off. Registration stays open, to show
/// that it doesn't matter.
fn off_app(app: &TestApp, provider: &FakeProvider) -> Result<TestApp> {
    Ok(app.with_settings(Settings {
        allow_registration: true,
        password_sign_in: false,
        oidc: Some(provider.settings()?),
        ..Settings::default()
    }))
}

/// A user's session, OAuth access token and personal token.
struct SignIns {
    session: String,
    access: String,
    personal: String,
}

/// A local user's sign-ins, made while passwords are on.
async fn local_sign_ins(on: &TestApp, email: &str) -> Result<SignIns> {
    let (session, _) = on.register(email).await?;
    let (access, _) = on.sign_in_cli(&session).await?;
    let made = on.make_personal_token(&session, "ci", None).await?;
    let personal = made.body["secret"]
        .as_str()
        .context("no personal token secret")?
        .to_owned();
    Ok(SignIns {
        session,
        access,
        personal,
    })
}

async fn me_status(app: &TestApp, sign_ins: &SignIns) -> Result<[StatusCode; 3]> {
    Ok([
        app.send(Method::GET, "/api/v1/me", Some(&sign_ins.session), None)
            .await?
            .status,
        app.send_bearer(Method::GET, "/api/v1/me", &sign_ins.access, None)
            .await?
            .status,
        app.send_bearer(Method::GET, "/api/v1/me", &sign_ins.personal, None)
            .await?
            .status,
    ])
}

fn expect_refused(response: &TestResponse) {
    expect_that!(response.status, eq(StatusCode::FORBIDDEN), "{}", response.body);
    expect_that!(response.body["code"], eq(&json!("password_sign_in_off")));
    expect_that!(
        response.body["title"],
        eq(&json!(format!("This server signs in through {LABEL}.")))
    );
}

#[gtest]
#[tokio::test]
async fn every_password_path_is_refused_with_the_provider_named() -> Result<()> {
    let h = harness().await?;
    let (ana, _) = h.on.register_as("ana@example.test", "Ana").await?;
    let team =
        h.on.send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    let team_url = h.on.team_url(team.body["id"].as_str().context("team id")?).await?;
    let link =
        h.on.send(Method::POST, &format!("{team_url}/invite-links"), Some(&ana), None)
            .await?;
    let invite = link.body["token"].as_str().context("invite token")?.to_owned();
    let reset = h.on.make_reset_link("ana").await?;
    let registration = json!({
        "email": "new@example.test",
        "username": "new",
        "display_name": "New",
        "password": PASSWORD,
    });

    let sign_in = h
        .off
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "login": "ana@example.test", "password": PASSWORD })),
        )
        .await?;
    let register = h
        .off
        .send(Method::POST, "/api/v1/auth/register", None, Some(registration.clone()))
        .await?;
    let invite_register = h
        .off
        .send(
            Method::POST,
            &format!("/api/v1/invites/{invite}/register"),
            None,
            Some(registration),
        )
        .await?;
    let request_reset = h
        .off
        .send(
            Method::POST,
            "/api/v1/password-resets",
            None,
            Some(json!({ "email": "ana@example.test" })),
        )
        .await?;
    let set_password = h
        .off
        .send(
            Method::POST,
            &format!("/api/v1/password-resets/{reset}"),
            None,
            Some(json!({ "password": "another long password" })),
        )
        .await?;

    for response in [&sign_in, &register, &invite_register, &request_reset, &set_password] {
        expect_refused(response);
    }
    expect_that!(h.off.count_rows("users").await?, eq(1));
    // The link was not used, so it works once passwords are on.
    let again =
        h.on.send(
            Method::POST,
            &format!("/api/v1/password-resets/{reset}"),
            None,
            Some(json!({ "password": "another long password" })),
        )
        .await?;
    expect_that!(again.status, eq(StatusCode::OK), "{}", again.body);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_auth_check_alone_refuses_a_local_users_sign_ins() -> Result<()> {
    let h = harness().await?;
    let sign_ins = local_sign_ins(&h.on, "sam@example.test").await?;
    expect_that!(me_status(&h.on, &sign_ins).await?, eq([StatusCode::OK; 3]));

    expect_that!(me_status(&h.off, &sign_ins).await?, eq([StatusCode::UNAUTHORIZED; 3]));
    let change = h
        .off
        .send(
            Method::POST,
            "/api/v1/me/password",
            Some(&sign_ins.session),
            Some(json!({ "current_password": PASSWORD, "new_password": "another long password" })),
        )
        .await?;
    expect_that!(change.status, eq(StatusCode::UNAUTHORIZED), "{}", change.body);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn ending_local_sign_ins_is_kept_when_passwords_come_back() -> Result<()> {
    let h = harness().await?;
    let sign_ins = local_sign_ins(&h.on, "lee@example.test").await?;
    // A provider user's sign-ins, which ending the local ones must leave alone.
    let session = provider_sign_in(&h.off, &h.provider, "/").await?;
    let (access, _) = h.off.sign_in_cli(&session).await?;
    let made = h.off.make_personal_token(&session, "ci", None).await?;
    let personal = made.body["secret"]
        .as_str()
        .context("no personal token secret")?
        .to_owned();
    let provider_sign_ins = SignIns {
        session,
        access,
        personal,
    };

    expect_that!(h.on.services.end_local_sign_ins().await?, eq(0));
    expect_that!(h.off.services.end_local_sign_ins().await?, eq(3));
    expect_that!(h.off.services.end_local_sign_ins().await?, eq(0));

    expect_that!(me_status(&h.on, &sign_ins).await?, eq([StatusCode::UNAUTHORIZED; 3]));
    expect_that!(me_status(&h.off, &provider_sign_ins).await?, eq([StatusCode::OK; 3]));
    let back =
        h.on.send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "login": "lee@example.test", "password": PASSWORD })),
        )
        .await?;
    expect_that!(back.status, eq(StatusCode::OK), "{}", back.body);
    Ok(())
}

/// Signs Sam in through the provider on `app`, with the username step, and returns the session.
async fn provider_sign_in(app: &TestApp, provider: &FakeProvider, redirect: &str) -> Result<String> {
    let started = app
        .send_with(
            Method::GET,
            &format!(
                "/api/v1/auth/oidc/start?redirect={}",
                form_urlencoded::byte_serialize(redirect.as_bytes()).collect::<String>()
            ),
            None,
            None,
            &[],
        )
        .await?;
    let location = started.headers.get(header::LOCATION).context("no Location")?.to_str()?;
    let request = oidc::parse_authorization(location)?;
    let state = request.state.clone();
    let code = provider.code_for(&request, Claims::verified("subject-sam", "sam@example.test", "Sam Doe"));
    let cookie = format!("upheap_oidc={state}");
    let callback = app
        .send_with(
            Method::GET,
            &format!("/api/v1/auth/oidc/callback?code={code}&state={state}"),
            None,
            None,
            &[("cookie", &cookie)],
        )
        .await?;
    let next = callback
        .headers
        .get(header::LOCATION)
        .context("no Location")?
        .to_str()?;
    assert_eq!(next, format!("{APP_URL}/login/provider"));
    let registered = app
        .send_with(
            Method::POST,
            "/api/v1/auth/oidc/register",
            None,
            Some(json!({ "username": "sam" })),
            &[CSRF, ("cookie", &cookie)],
        )
        .await?;
    assert_eq!(registered.status, StatusCode::CREATED, "{}", registered.body);
    registered.session_cookie.context("no session cookie")
}

#[gtest]
#[tokio::test]
async fn a_provider_user_signs_in_and_joins_a_team_by_invite() -> Result<()> {
    let h = harness().await?;
    let (ana, _) = h.on.register_as("ana@example.test", "Ana").await?;
    let team =
        h.on.send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    let team_url = h.on.team_url(team.body["id"].as_str().context("team id")?).await?;
    let link =
        h.on.send(Method::POST, &format!("{team_url}/invite-links"), Some(&ana), None)
            .await?;
    let invite = link.body["token"].as_str().context("invite token")?.to_owned();

    let session = provider_sign_in(&h.off, &h.provider, &format!("/invite/{invite}")).await?;

    let me = h.off.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    expect_that!(me.status, eq(StatusCode::OK), "{}", me.body);
    let shown = h
        .off
        .send(Method::GET, &format!("/api/v1/invites/{invite}"), None, None)
        .await?;
    expect_that!(shown.status, eq(StatusCode::OK), "{}", shown.body);
    let joined = h
        .off
        .send(
            Method::POST,
            &format!("/api/v1/invites/{invite}/join"),
            Some(&session),
            None,
        )
        .await?;
    expect_that!(joined.status, eq(StatusCode::CREATED), "{}", joined.body);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_provider_data_says_whether_passwords_are_on() -> Result<()> {
    let h = harness().await?;
    let on = h.on.with_oidc(h.provider.settings()?);

    let off = h.off.send(Method::GET, "/api/v1/auth/oidc", None, None).await?;
    let still_on = on.send(Method::GET, "/api/v1/auth/oidc", None, None).await?;

    expect_that!(off.body["password_sign_in"], eq(&Value::Bool(false)));
    expect_that!(still_on.body["password_sign_in"], eq(&Value::Bool(true)));
    Ok(())
}
