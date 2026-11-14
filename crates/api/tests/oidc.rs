//! Signing in through an OpenID Connect provider, against a fake one: the start, the callback, the username step,
//! and every way a sign-in is refused.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode, header};
use common::oidc::{self, APP_URL, AuthorizationRequest, Claims, FakeProvider, LABEL, TokenFault};
use common::{CSRF, TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

const EMAIL: &str = "sam@example.test";

struct Harness {
    provider: FakeProvider,
    /// The server with the provider and registration closed.
    app: TestApp,
    /// The same server with no provider.
    plain: TestApp,
}

async fn harness() -> Result<Harness> {
    let provider = FakeProvider::start().await?;
    let plain = TestApp::start().await?;
    let app = plain.with_oidc(provider.settings()?);
    Ok(Harness { provider, app, plain })
}

/// A sign-in the start began.
struct Started {
    request: AuthorizationRequest,
    response: TestResponse,
}

fn location(response: &TestResponse) -> Result<String> {
    Ok(response
        .headers
        .get(header::LOCATION)
        .context("no Location")?
        .to_str()?
        .to_owned())
}

/// The `Set-Cookie` values for `name`.
fn set_cookies(response: &TestResponse, name: &str) -> Vec<String> {
    response
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter(|value| value.starts_with(&format!("{name}=")))
        .map(str::to_owned)
        .collect()
}

fn encode(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

async fn start(app: &TestApp, redirect: Option<&str>) -> Result<TestResponse> {
    let path = match redirect {
        Some(redirect) => format!("/api/v1/auth/oidc/start?redirect={}", encode(redirect)),
        None => "/api/v1/auth/oidc/start".to_owned(),
    };
    app.send_with(Method::GET, &path, None, None, &[]).await
}

async fn begin(app: &TestApp, redirect: Option<&str>) -> Result<Started> {
    let response = start(app, redirect).await?;
    assert_eq!(response.status, StatusCode::SEE_OTHER, "{}", response.body);
    let request = oidc::parse_authorization(&location(&response)?)?;
    Ok(Started { request, response })
}

async fn callback(app: &TestApp, cookie_state: Option<&str>, query: &str) -> Result<TestResponse> {
    let cookie = cookie_state.map(|state| format!("upheap_oidc={state}"));
    let headers: Vec<(&str, &str)> = cookie.iter().map(|cookie| ("cookie", cookie.as_str())).collect();
    app.send_with(
        Method::GET,
        &format!("/api/v1/auth/oidc/callback?{query}"),
        None,
        None,
        &headers,
    )
    .await
}

/// The callback after the provider vouched for `claims` in the sign-in `started`.
async fn finish(h: &Harness, started: &Started, claims: Claims) -> Result<TestResponse> {
    let state = &started.request.state;
    let code = h.provider.code_for(&started.request, claims);
    callback(&h.app, Some(state), &format!("code={code}&state={state}")).await
}

async fn pending(app: &TestApp, state: &str) -> Result<TestResponse> {
    app.send_with(
        Method::GET,
        "/api/v1/auth/oidc/pending",
        None,
        None,
        &[("cookie", &format!("upheap_oidc={state}"))],
    )
    .await
}

async fn register(app: &TestApp, state: &str, username: &str) -> Result<TestResponse> {
    app.send_with(
        Method::POST,
        "/api/v1/auth/oidc/register",
        None,
        Some(json!({ "username": username })),
        &[CSRF, ("cookie", &format!("upheap_oidc={state}"))],
    )
    .await
}

fn sam() -> Claims {
    Claims::verified("subject-sam", EMAIL, "Sam Doe")
}

/// A first sign-in as Sam, through the username step, as `username`. Returns the registered response.
async fn sign_up(h: &Harness, redirect: Option<&str>, username: &str) -> Result<TestResponse> {
    let started = begin(&h.app, redirect).await?;
    let callback = finish(h, &started, sam()).await?;
    assert_eq!(location(&callback)?, format!("{APP_URL}/login/provider"));
    let registered = register(&h.app, &started.request.state, username).await?;
    assert_eq!(registered.status, StatusCode::CREATED, "{}", registered.body);
    Ok(registered)
}

async fn me(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(Method::GET, "/api/v1/me", Some(session), None).await
}

fn can_change_password(me: &TestResponse) -> bool {
    me.body["allowed_actions"]
        .as_array()
        .is_some_and(|actions| actions.contains(&json!("change_password")))
}

fn refusal_location(code: &str) -> String {
    format!("{APP_URL}/login?sign_in_error={code}")
}

#[gtest]
#[tokio::test]
async fn a_server_without_a_provider_has_no_sign_in_through_one() -> Result<()> {
    let app = TestApp::start().await?;
    let info = app.send(Method::GET, "/api/v1/auth/oidc", None, None).await?;
    expect_that!(info.status, eq(StatusCode::NOT_FOUND));
    expect_that!(start(&app, None).await?.status, eq(StatusCode::NOT_FOUND));
    let callback = callback(&app, None, "code=x&state=y").await?;
    expect_that!(callback.status, eq(StatusCode::NOT_FOUND));

    let (session, _) = app.register("ada@example.test").await?;
    let me = me(&app, &session).await?;
    expect_that!(can_change_password(&me), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_new_person_picks_a_username_even_when_registration_is_closed() -> Result<()> {
    let h = harness().await?;
    let info = h.app.send(Method::GET, "/api/v1/auth/oidc", None, None).await?;
    expect_that!(info.body, eq(&json!({ "label": LABEL })));

    let started = begin(&h.app, None).await?;
    let cookies = set_cookies(&started.response, "upheap_oidc");
    expect_that!(
        cookies,
        elements_are![eq(&format!(
            "upheap_oidc={}; Path=/api/v1/auth/oidc; HttpOnly; SameSite=Lax; Max-Age=900; Secure",
            started.request.state
        ))]
    );
    expect_that!(
        location(&started.response)?,
        starts_with(format!("{}authorize?", h.provider.issuer))
    );

    let answered = finish(&h, &started, sam()).await?;
    expect_that!(answered.status, eq(StatusCode::SEE_OTHER));
    expect_that!(location(&answered)?, eq(&format!("{APP_URL}/login/provider")));
    expect_that!(set_cookies(&answered, "upheap_oidc"), is_empty());
    expect_that!(answered.session_cookie, none());

    let shown = pending(&h.app, &started.request.state).await?;
    expect_that!(shown.body, eq(&json!({ "display_name": "Sam Doe", "email": EMAIL })));

    let registered = register(&h.app, &started.request.state, "sam").await?;
    expect_that!(registered.status, eq(StatusCode::CREATED));
    expect_that!(registered.body["redirect"], eq(&Value::Null));
    expect_that!(registered.body["user"]["email"], eq(&json!(EMAIL)));
    expect_that!(
        set_cookies(&registered, "upheap_oidc"),
        elements_are![contains_substring("Max-Age=0")]
    );
    let session = registered.session_cookie.context("no session cookie")?;
    let me = me(&h.app, &session).await?;
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(me.body["teams"][0]["slug"], eq(&json!("sam")));
    expect_that!(me.body["teams"][0]["heaps"][0]["slug"], eq(&json!("personal")));
    expect_that!(can_change_password(&me), eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_known_person_is_signed_in_and_their_details_are_left_as_they_are() -> Result<()> {
    let h = harness().await?;
    sign_up(&h, None, "sam").await?;
    let users = h.app.count_rows("users").await?;

    let started = begin(&h.app, None).await?;
    let changed = Claims::verified("subject-sam", "other@example.test", "Someone Else");
    let answered = finish(&h, &started, changed).await?;
    expect_that!(answered.status, eq(StatusCode::SEE_OTHER));
    expect_that!(location(&answered)?, eq(&format!("{APP_URL}/")));
    expect_that!(
        set_cookies(&answered, "upheap_oidc"),
        elements_are![contains_substring("Max-Age=0")]
    );
    let session = answered.session_cookie.context("no session cookie")?;
    let me = me(&h.app, &session).await?;
    expect_that!(me.body["user"]["email"], eq(&json!(EMAIL)));
    expect_that!(me.body["user"]["display_name"], eq(&json!("Sam Doe")));
    expect_that!(h.app.count_rows("users").await?, eq(users));
    expect_that!(h.app.count_rows("oidc_sign_ins").await?, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_sign_in_from_an_invite_page_comes_back_to_it_and_the_session_can_join() -> Result<()> {
    let h = harness().await?;
    let (ana, _) = h.plain.register_as("ana@example.test", "Ana").await?;
    let team = h
        .plain
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    let team_url = h.plain.team_url(team.body["id"].as_str().context("team id")?).await?;
    let link = h
        .plain
        .send(Method::POST, &format!("{team_url}/invite-links"), Some(&ana), None)
        .await?;
    let token = link.body["token"].as_str().context("token")?;
    let invite_path = format!("/invite/{token}");

    let registered = sign_up(&h, Some(&invite_path), "sam").await?;
    expect_that!(registered.body["redirect"], eq(&json!(invite_path)));
    let session = registered.session_cookie.context("no session cookie")?;
    let joined = h
        .app
        .send(
            Method::POST,
            &format!("/api/v1/invites/{token}/join"),
            Some(&session),
            None,
        )
        .await?;
    expect_that!(joined.status, eq(StatusCode::CREATED), "{}", joined.body);
    let me = me(&h.app, &session).await?;
    expect_that!(me.body["teams"].as_array().map(Vec::len), some(eq(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_email_of_a_local_account_is_refused_and_nothing_is_made() -> Result<()> {
    let h = harness().await?;
    h.plain.register_as(EMAIL, "Local Sam").await?;
    let users = h.app.count_rows("users").await?;

    let started = begin(&h.app, Some("/invite/abc")).await?;
    let answered = finish(&h, &started, sam()).await?;
    expect_that!(answered.status, eq(StatusCode::SEE_OTHER));
    expect_that!(
        location(&answered)?,
        eq(&format!(
            "{}&redirect=%2Finvite%2Fabc&login=sam%40example.test",
            refusal_location("email_taken")
        ))
    );
    expect_that!(
        set_cookies(&answered, "upheap_oidc"),
        elements_are![contains_substring("Max-Age=0")]
    );
    expect_that!(h.app.count_rows("users").await?, eq(users));
    expect_that!(h.app.count_rows("user_identities").await?, eq(0));
    expect_that!(h.app.count_rows("oidc_sign_ins").await?, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_email_the_provider_doesnt_vouch_for_is_refused() -> Result<()> {
    let h = harness().await?;
    let unverified = Claims {
        email_verified: Some(false),
        ..sam()
    };
    let missing_flag = Claims {
        email_verified: None,
        ..sam()
    };
    let no_email = Claims {
        email: None,
        email_verified: None,
        ..sam()
    };
    for claims in [unverified, missing_flag, no_email] {
        let started = begin(&h.app, None).await?;
        let answered = finish(&h, &started, claims).await?;
        expect_that!(location(&answered)?, eq(&refusal_location("email_unverified")));
    }
    expect_that!(h.app.count_rows("user_identities").await?, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_sign_in_the_provider_refused_or_that_took_too_long_is_refused() -> Result<()> {
    let h = harness().await?;

    let started = begin(&h.app, None).await?;
    let state = &started.request.state;
    let denied = callback(&h.app, Some(state), &format!("error=access_denied&state={state}")).await?;
    expect_that!(location(&denied)?, eq(&refusal_location("provider_refused")));
    expect_that!(h.app.count_rows("oidc_sign_ins").await?, eq(0));

    let started = begin(&h.app, None).await?;
    let code = h.provider.code_for(&started.request, sam());
    let answered = callback(
        &h.app,
        Some("another-browser"),
        &format!("code={code}&state={}", started.request.state),
    )
    .await?;
    expect_that!(location(&answered)?, eq(&refusal_location("sign_in_expired")));
    let no_cookie = callback(&h.app, None, &format!("code={code}&state={}", started.request.state)).await?;
    expect_that!(location(&no_cookie)?, eq(&refusal_location("sign_in_expired")));

    let started = begin(&h.app, None).await?;
    h.app.clock.advance(Duration::from_minutes(16));
    let late = finish(&h, &started, sam()).await?;
    expect_that!(location(&late)?, eq(&refusal_location("sign_in_expired")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_id_token_that_should_be_rejected_is_refused_and_nothing_is_made() -> Result<()> {
    let h = harness().await?;
    let faults = [
        (
            "wrong nonce",
            TokenFault {
                nonce: Some("another-nonce".to_owned()),
                ..TokenFault::default()
            },
        ),
        (
            "wrong audience",
            TokenFault {
                audience: Some("another-client".to_owned()),
                ..TokenFault::default()
            },
        ),
        (
            "wrong issuer",
            TokenFault {
                issuer: Some("https://another-issuer.example.test/".to_owned()),
                ..TokenFault::default()
            },
        ),
        (
            "expired",
            TokenFault {
                // Real time: openidconnect doesn't read the server's clock.
                expires_in: Some(chrono::Duration::hours(-1)),
                ..TokenFault::default()
            },
        ),
        (
            "bad signature",
            TokenFault {
                wrong_key: true,
                ..TokenFault::default()
            },
        ),
    ];
    for (name, fault) in faults {
        let started = begin(&h.app, None).await?;
        let answered = finish(&h, &started, Claims { fault, ..sam() }).await?;
        expect_that!(answered.status, eq(StatusCode::SEE_OTHER), "{name}");
        expect_that!(
            location(&answered)?,
            eq(&refusal_location("provider_refused")),
            "{name}"
        );
        expect_that!(
            answered.headers.get_all(header::SET_COOKIE).iter().count(),
            eq(1),
            "{name}: only the state cookie is ended"
        );
        expect_that!(h.app.count_rows("user_identities").await?, eq(0), "{name}");
        expect_that!(h.app.count_rows("users").await?, eq(0), "{name}");
        expect_that!(h.app.count_rows("sessions").await?, eq(0), "{name}");
        expect_that!(h.app.count_rows("oidc_sign_ins").await?, eq(0), "{name}");
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_username_step_ends_sixteen_minutes_after_the_callback() -> Result<()> {
    let h = harness().await?;
    let started = begin(&h.app, None).await?;
    finish(&h, &started, sam()).await?;
    let state = &started.request.state;
    expect_that!(pending(&h.app, state).await?.status, eq(StatusCode::OK));

    h.app.clock.advance(Duration::from_minutes(16));
    expect_that!(pending(&h.app, state).await?.status, eq(StatusCode::NOT_FOUND));
    let registered = register(&h.app, state, "sam").await?;
    expect_that!(registered.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_sign_in_that_wasnt_verified_has_no_username_step() -> Result<()> {
    let h = harness().await?;
    let started = begin(&h.app, None).await?;
    let state = &started.request.state;
    expect_that!(pending(&h.app, state).await?.status, eq(StatusCode::NOT_FOUND));
    expect_that!(register(&h.app, state, "sam").await?.status, eq(StatusCode::NOT_FOUND));
    let no_cookie = h.app.send(Method::GET, "/api/v1/auth/oidc/pending", None, None).await?;
    expect_that!(no_cookie.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_cli_authorization_request_is_the_page_to_return_to() -> Result<()> {
    let h = harness().await?;
    let path = format!(
        "/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state=af0ifjsldkj",
        common::CLI_CLIENT,
        encode(common::CLI_REDIRECT),
        common::PKCE_CHALLENGE
    );

    let registered = sign_up(&h, Some(&path), "sam").await?;
    expect_that!(registered.body["redirect"], eq(&json!(path)));

    let started = begin(&h.app, Some(&path)).await?;
    let answered = finish(&h, &started, sam()).await?;
    expect_that!(location(&answered)?, eq(&format!("{APP_URL}{path}")));

    let started = begin(&h.app, Some("//evil.example")).await?;
    let answered = finish(&h, &started, sam()).await?;
    expect_that!(location(&answered)?, eq(&format!("{APP_URL}/")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bad_username_is_refused_and_the_sign_in_waits_for_another() -> Result<()> {
    let h = harness().await?;
    h.plain.register("sam@other.test").await?;

    let started = begin(&h.app, None).await?;
    finish(&h, &started, sam()).await?;
    let state = &started.request.state;

    let empty = register(&h.app, state, "").await?;
    expect_that!(
        (empty.status, &empty.body["code"]),
        eq((StatusCode::UNPROCESSABLE_ENTITY, &json!("invalid_handle")))
    );
    let invalid = register(&h.app, state, "Not Valid!").await?;
    expect_that!(
        (invalid.status, &invalid.body["code"]),
        eq((StatusCode::UNPROCESSABLE_ENTITY, &json!("invalid_handle")))
    );
    let taken = register(&h.app, state, "sam").await?;
    expect_that!(
        (taken.status, &taken.body["code"], &taken.body["field"]),
        eq((StatusCode::CONFLICT, &json!("handle_taken"), &json!("username")))
    );
    expect_that!(h.app.count_rows("user_identities").await?, eq(0));

    let made = register(&h.app, state, "sam-doe").await?;
    expect_that!(made.status, eq(StatusCode::CREATED));
    let again = register(&h.app, state, "sam-doe-2").await?;
    expect_that!(again.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn register_needs_the_csrf_header() -> Result<()> {
    let h = harness().await?;
    let started = begin(&h.app, None).await?;
    finish(&h, &started, sam()).await?;
    let response = h
        .app
        .send_with(
            Method::POST,
            "/api/v1/auth/oidc/register",
            None,
            Some(json!({ "username": "sam" })),
            &[("cookie", &format!("upheap_oidc={}", started.request.state))],
        )
        .await?;
    expect_that!(response.status, eq(StatusCode::FORBIDDEN));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_person_from_the_provider_has_no_password_to_sign_in_with() -> Result<()> {
    let h = harness().await?;
    sign_up(&h, None, "sam").await?;
    for login in [EMAIL, "sam"] {
        let response = h
            .app
            .send(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "login": login, "password": "a long test password" })),
            )
            .await?;
        expect_that!(response.status, eq(StatusCode::UNAUTHORIZED));
        expect_that!(response.body["code"], eq(&json!("invalid_credentials")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_provider_that_cant_be_reached_sends_the_person_back_to_sign_in() -> Result<()> {
    let h = harness().await?;
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let issuer = format!("http://{}/", closed.local_addr()?);
    drop(closed);
    let app = h.plain.with_oidc(oidc::settings_for(&issuer)?);

    let response = start(&app, Some("/invite/abc")).await?;
    expect_that!(response.status, eq(StatusCode::SEE_OTHER));
    expect_that!(
        location(&response)?,
        eq(&format!(
            "{}&redirect=%2Finvite%2Fabc",
            refusal_location("provider_unreachable")
        ))
    );
    expect_that!(app.count_rows("oidc_sign_ins").await?, eq(0));
    Ok(())
}
