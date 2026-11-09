//! Signing in the CLI: the authorization code flow with PKCE, refresh token rotation, revocation and bearer tokens.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode, header};
use common::{CLI_CLIENT, CLI_REDIRECT, CSRF, PKCE_VERIFIER, TestApp, TestResponse, cli_authorization};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

async fn refresh(app: &TestApp, refresh_token: &str) -> Result<TestResponse> {
    app.post_form(
        "/api/v1/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLI_CLIENT),
        ],
    )
    .await
}

async fn revoke(app: &TestApp, token: &str) -> Result<TestResponse> {
    app.post_form("/api/v1/oauth/revoke", &[("token", token), ("client_id", CLI_CLIENT)])
        .await
}

fn header_of(response: &TestResponse, name: header::HeaderName) -> Option<&str> {
    response.headers.get(name).and_then(|v| v.to_str().ok())
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// `cli_authorization()` with `field` set to `value`, as a query string for `GET /oauth/authorize`.
fn authorization_query(field: &str, value: &str) -> String {
    let mut request = cli_authorization();
    request[field] = json!(value);
    let pairs = request
        .as_object()
        .map(|o| o.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    pairs
        .into_iter()
        .map(|(name, value)| {
            let value: String = value
                .as_str()
                .unwrap_or_default()
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() {
                        char::from(b).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect();
            format!("{name}={value}")
        })
        .collect::<Vec<_>>()
        .join("&")
}

#[gtest]
#[tokio::test]
async fn the_cli_signs_in_reads_with_its_token_and_rotates_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;

    // Act: the consent page approves, and the CLI exchanges the code.
    let approved = app
        .send(
            Method::POST,
            "/api/v1/oauth/authorize",
            Some(&session),
            Some(cli_authorization()),
        )
        .await?;
    let code = app.approve_cli(&session).await?;
    let tokens = app.exchange_code(&code).await?;
    let access = text(&tokens.body["access_token"]);
    let refresh_token = text(&tokens.body["refresh_token"]);
    let me = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;
    let refreshed = refresh(&app, &refresh_token).await?;
    let new_access = text(&refreshed.body["access_token"]);
    let me_after_refresh = app.send_bearer(Method::GET, "/api/v1/me", &new_access, None).await?;

    // Assert
    expect_that!(approved.status, eq(StatusCode::OK));
    expect_that!(
        text(&approved.body["redirect_to"]),
        matches_regex(r"^http://127\.0\.0\.1:53682/callback\?code=[A-Za-z0-9_-]{43}&state=af0ifjsldkj$")
    );
    expect_that!(tokens.status, eq(StatusCode::OK));
    expect_that!(tokens.body["token_type"], eq(&json!("Bearer")));
    expect_that!(tokens.body["expires_in"], eq(&json!(900)));
    expect_that!(header_of(&tokens, header::CACHE_CONTROL), some(eq("no-store")));
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(me.body["user"]["username"], eq(&json!("ada")));
    expect_that!(refreshed.status, eq(StatusCode::OK));
    expect_that!(text(&refreshed.body["refresh_token"]), not(eq(&refresh_token)));
    expect_that!(me_after_refresh.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reused_refresh_token_revokes_every_token_of_its_sign_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (first_access, first_refresh) = app.sign_in_cli(&session).await?;
    let refreshed = refresh(&app, &first_refresh).await?;
    let (second_access, second_refresh) = (
        text(&refreshed.body["access_token"]),
        text(&refreshed.body["refresh_token"]),
    );

    // Act
    let reused = refresh(&app, &first_refresh).await?;
    let first_me = app.send_bearer(Method::GET, "/api/v1/me", &first_access, None).await?;
    let second_me = app.send_bearer(Method::GET, "/api/v1/me", &second_access, None).await?;
    let second_refreshed = refresh(&app, &second_refresh).await?;

    // Assert
    expect_that!(reused.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(reused.body["error"], eq(&json!("invalid_grant")));
    expect_that!(
        text(&reused.body["error_description"]),
        contains_substring("used already")
    );
    for revoked in [&first_me, &second_me] {
        expect_that!(revoked.status, eq(StatusCode::UNAUTHORIZED));
        expect_that!(revoked.body["code"], eq(&json!("unauthenticated")));
        expect_that!(
            header_of(revoked, header::WWW_AUTHENTICATE),
            some(eq(r#"Bearer error="invalid_token""#))
        );
    }
    expect_that!(second_refreshed.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(second_refreshed.body["error"], eq(&json!("invalid_grant")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn revoking_either_token_ends_the_sign_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;
    let (other_access, other_refresh) = app.sign_in_cli(&session).await?;

    // Act
    let revoked = revoke(&app, &refresh_token).await?;
    let again = revoke(&app, &refresh_token).await?;
    let by_access = revoke(&app, &other_access).await?;
    let me = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;
    let refreshed = refresh(&app, &refresh_token).await?;
    let other_refreshed = refresh(&app, &other_refresh).await?;

    // Assert
    expect_that!(revoked.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(by_access.status, eq(StatusCode::OK));
    expect_that!(me.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(refreshed.body["error"], eq(&json!("invalid_grant")));
    expect_that!(other_refreshed.body["error"], eq(&json!("invalid_grant")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn revoking_an_unknown_token_succeeds_and_a_missing_one_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let unknown = revoke(&app, "not-a-token").await?;
    let missing = app
        .post_form("/api/v1/oauth/revoke", &[("client_id", CLI_CLIENT)])
        .await?;
    let unknown_client = app
        .post_form("/api/v1/oauth/revoke", &[("token", "x"), ("client_id", "other")])
        .await?;

    // Assert
    expect_that!(unknown.status, eq(StatusCode::OK));
    expect_that!(missing.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(missing.body["error"], eq(&json!("invalid_request")));
    expect_that!(unknown_client.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(unknown_client.body["error"], eq(&json!("invalid_client")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_code_needs_its_own_verifier_and_redirect_uri() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let code = app.approve_cli(&session).await?;
    let exchange = |redirect_uri: &'static str, verifier: &'static str| {
        let code = code.clone();
        let app = &app;
        async move {
            app.post_form(
                "/api/v1/oauth/token",
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", redirect_uri),
                    ("client_id", CLI_CLIENT),
                    ("code_verifier", verifier),
                ],
            )
            .await
        }
    };

    // Act
    let wrong_verifier = exchange(CLI_REDIRECT, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXl").await?;
    let short_verifier = exchange(CLI_REDIRECT, "dBjftJeZ4CVP").await?;
    let wrong_redirect = exchange("http://127.0.0.1:53683/callback", PKCE_VERIFIER).await?;
    // Failed exchanges don't use the code up.
    let right = app.exchange_code(&code).await?;

    // Assert
    for refused in [&wrong_verifier, &short_verifier] {
        expect_that!(refused.status, eq(StatusCode::BAD_REQUEST));
        expect_that!(refused.body["error"], eq(&json!("invalid_grant")));
        expect_that!(
            text(&refused.body["error_description"]),
            contains_substring("code_verifier")
        );
    }
    expect_that!(wrong_redirect.body["error"], eq(&json!("invalid_grant")));
    expect_that!(
        text(&wrong_redirect.body["error_description"]),
        contains_substring("redirect_uri")
    );
    expect_that!(header_of(&wrong_redirect, header::CACHE_CONTROL), some(eq("no-store")));
    expect_that!(right.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_code_works_once_and_a_second_try_revokes_what_the_first_got() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let code = app.approve_cli(&session).await?;
    let first = app.exchange_code(&code).await?;

    // Act
    let second = app.exchange_code(&code).await?;
    let me = app
        .send_bearer(Method::GET, "/api/v1/me", &text(&first.body["access_token"]), None)
        .await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(second.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(second.body["error"], eq(&json!("invalid_grant")));
    expect_that!(me.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_code_reused_after_its_lifetime_is_refused_and_revokes_nothing() -> Result<()> {
    // Arrange: the code stays in the browser's history after the CLI exchanged it.
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let code = app.approve_cli(&session).await?;
    let first = app.exchange_code(&code).await?;
    app.clock.advance(Duration::from_minutes(11));

    // Act
    let again = app.exchange_code(&code).await?;
    let me = app
        .send_bearer(Method::GET, "/api/v1/me", &text(&first.body["access_token"]), None)
        .await?;
    let refreshed = refresh(&app, &text(&first.body["refresh_token"])).await?;

    // Assert
    expect_that!(again.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(again.body["error"], eq(&json!("invalid_grant")));
    expect_that!(me.status, eq(StatusCode::OK));
    expect_that!(refreshed.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn codes_and_access_tokens_expire() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let stale_code = app.approve_cli(&session).await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;

    // Act
    app.clock.advance(Duration::from_minutes(15));
    let exchanged = app.exchange_code(&stale_code).await?;
    let me = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;
    let refreshed = refresh(&app, &refresh_token).await?;
    app.clock.advance(Duration::from_days(31));
    let refreshed_late = refresh(&app, &text(&refreshed.body["refresh_token"])).await?;

    // Assert
    expect_that!(exchanged.body["error"], eq(&json!("invalid_grant")));
    expect_that!(
        text(&exchanged.body["error_description"]),
        contains_substring("expired")
    );
    expect_that!(me.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(refreshed.status, eq(StatusCode::OK));
    expect_that!(refreshed_late.body["error"], eq(&json!("invalid_grant")));
    expect_that!(
        text(&refreshed_late.body["error_description"]),
        contains_substring("expired")
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_consent_page_can_check_a_request_before_approving_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let good = app
        .send(
            Method::GET,
            &format!("/api/v1/oauth/authorize?{}", authorization_query("state", "s")),
            None,
            None,
        )
        .await?;

    // Assert
    expect_that!(good.status, eq(StatusCode::OK));
    expect_that!(
        good.body,
        eq(&json!({ "client_id": "upheap-cli", "display_name": "Upheap CLI" }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn unregistered_clients_and_non_loopback_redirect_uris_are_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let cases = [
        ("client_id", "someone-else", "client_unknown"),
        (
            "redirect_uri",
            "http://localhost:53682/callback",
            "redirect_uri_invalid",
        ),
        (
            "redirect_uri",
            "https://127.0.0.1:53682/callback",
            "redirect_uri_invalid",
        ),
        (
            "redirect_uri",
            "http://evil.example:53682/callback",
            "redirect_uri_invalid",
        ),
        (
            "redirect_uri",
            "http://127.0.0.1:53682/callback#x",
            "redirect_uri_invalid",
        ),
        (
            "redirect_uri",
            "http://127.0.0.1:53682/callback?next=x",
            "redirect_uri_invalid",
        ),
        ("response_type", "token", "response_type_unsupported"),
        ("code_challenge", "", "code_challenge_invalid"),
        ("code_challenge_method", "plain", "code_challenge_method_unsupported"),
        ("state", "", "state_missing"),
    ];
    let mut mismatches = Vec::new();

    // Act
    for (field, value, code) in cases {
        let checked = app
            .send(
                Method::GET,
                &format!("/api/v1/oauth/authorize?{}", authorization_query(field, value)),
                None,
                None,
            )
            .await?;
        let mut body = cli_authorization();
        body[field] = json!(value);
        let approved = app
            .send(Method::POST, "/api/v1/oauth/authorize", Some(&session), Some(body))
            .await?;
        for response in [checked, approved] {
            let got = (
                response.status,
                text(&response.body["code"]),
                text(&response.body["field"]),
            );
            if got != (StatusCode::UNPROCESSABLE_ENTITY, code.to_owned(), field.to_owned()) {
                mismatches.push(format!("{field}={value:?}: {got:?}"));
            }
        }
    }

    // Assert
    expect_that!(mismatches, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn token_requests_are_refused_in_the_rfc_format() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let unknown_client = app
        .post_form(
            "/api/v1/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", "x"),
                ("client_id", "other"),
            ],
        )
        .await?;
    let no_client = app
        .post_form(
            "/api/v1/oauth/token",
            &[("grant_type", "refresh_token"), ("refresh_token", "x")],
        )
        .await?;
    let password_grant = app
        .post_form(
            "/api/v1/oauth/token",
            &[("grant_type", "password"), ("client_id", CLI_CLIENT)],
        )
        .await?;
    let no_code = app
        .post_form(
            "/api/v1/oauth/token",
            &[("grant_type", "authorization_code"), ("client_id", CLI_CLIENT)],
        )
        .await?;
    let unknown_refresh = refresh(&app, "not-a-token").await?;
    let json_body = app
        .send_with(
            Method::POST,
            "/api/v1/oauth/token",
            None,
            Some(json!({ "grant_type": "refresh_token" })),
            &[],
        )
        .await?;

    // Assert
    expect_that!(unknown_client.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(unknown_client.body["error"], eq(&json!("invalid_client")));
    expect_that!(no_client.body["error"], eq(&json!("invalid_request")));
    expect_that!(password_grant.body["error"], eq(&json!("unsupported_grant_type")));
    expect_that!(no_code.body["error"], eq(&json!("invalid_request")));
    expect_that!(unknown_refresh.body["error"], eq(&json!("invalid_grant")));
    expect_that!(json_body.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(json_body.body["error"], eq(&json!("invalid_request")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bearer_token_writes_without_the_csrf_header_and_a_cookie_still_needs_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;
    let team = json!({ "name": "From the terminal" });

    // Act
    let by_bearer = app
        .send_bearer(Method::POST, "/api/v1/teams", &access, Some(team.clone()))
        .await?;
    let by_cookie = app
        .send_with(Method::POST, "/api/v1/teams", Some(&session), Some(team), &[])
        .await?;
    let refresh_as_bearer = app.send_bearer(Method::GET, "/api/v1/me", &refresh_token, None).await?;

    // Assert
    expect_that!(by_bearer.status, eq(StatusCode::CREATED));
    expect_that!(by_cookie.status, eq(StatusCode::FORBIDDEN));
    expect_that!(by_cookie.body["code"], eq(&json!("csrf_header_missing")));
    expect_that!(refresh_as_bearer.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bearer_header_is_all_that_counts_however_bad() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (access, _) = app.sign_in_cli(&session).await?;
    let mut refused = Vec::new();

    // Act: a good cookie doesn't rescue a bad bearer header, and a token can't approve more access for itself.
    for bad in ["Bearer not-a-token", "Bearer", "bearer ", "BEARER two words"] {
        let response = app
            .send_with(
                Method::GET,
                "/api/v1/me",
                Some(&session),
                None,
                &[("authorization", bad)],
            )
            .await?;
        refused.push((bad, response));
    }
    let authorization = format!("Bearer {access}");
    let approve_with_token = app
        .send_with(
            Method::POST,
            "/api/v1/oauth/authorize",
            Some(&session),
            Some(cli_authorization()),
            &[CSRF, ("authorization", &authorization)],
        )
        .await?;
    let lowercase = format!("bearer {access}");
    let lowercase_scheme = app
        .send_with(Method::GET, "/api/v1/me", None, None, &[("authorization", &lowercase)])
        .await?;

    // Assert
    for (bad, response) in &refused {
        expect_that!(response.status, eq(StatusCode::UNAUTHORIZED), "{bad:?}");
        expect_that!(
            header_of(response, header::WWW_AUTHENTICATE),
            some(eq(r#"Bearer error="invalid_token""#)),
            "{bad:?}"
        );
    }
    expect_that!(approve_with_token.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(approve_with_token.body["code"], eq(&json!("unauthenticated")));
    expect_that!(lowercase_scheme.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn other_authorization_schemes_leave_the_cookie_and_the_csrf_check_as_they_are() -> Result<()> {
    // Arrange: a browser behind a proxy with `auth_basic` sends Basic credentials with every request, cross-site
    // ones included.
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let basic = ("authorization", "Basic YTpi");

    // Act
    let me = app
        .send_with(Method::GET, "/api/v1/me", Some(&session), None, &[basic])
        .await?;
    let write_without_csrf = app
        .send_with(
            Method::POST,
            "/api/v1/teams",
            Some(&session),
            Some(json!({ "name": "Forged" })),
            &[basic],
        )
        .await?;
    let logout_without_csrf = app
        .send_with(Method::POST, "/api/v1/auth/logout", Some(&session), None, &[basic])
        .await?;
    let write_with_csrf = app
        .send_with(
            Method::POST,
            "/api/v1/teams",
            Some(&session),
            Some(json!({ "name": "Real" })),
            &[basic, CSRF],
        )
        .await?;

    // Assert
    expect_that!(me.status, eq(StatusCode::OK));
    for forged in [&write_without_csrf, &logout_without_csrf] {
        expect_that!(forged.status, eq(StatusCode::FORBIDDEN));
        expect_that!(forged.body["code"], eq(&json!("csrf_header_missing")));
    }
    expect_that!(write_with_csrf.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn signing_out_of_the_web_app_leaves_a_bearer_token_alone() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (access, _) = app.sign_in_cli(&session).await?;

    // Act
    let logout = app
        .send_bearer(Method::POST, "/api/v1/auth/logout", &access, None)
        .await?;
    let me = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;

    // Assert
    expect_that!(logout.status, eq(StatusCode::NO_CONTENT));
    expect_that!(me.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn approving_needs_a_session() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;

    // Act
    let anonymous = app
        .send(Method::POST, "/api/v1/oauth/authorize", None, Some(cli_authorization()))
        .await?;

    // Assert
    expect_that!(anonymous.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(anonymous.body["code"], eq(&json!("unauthenticated")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_local_build_has_no_oauth() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;

    // Act
    let checked = app
        .send(
            Method::GET,
            &format!("/api/v1/oauth/authorize?{}", authorization_query("state", "s")),
            None,
            None,
        )
        .await?;
    let token = app.exchange_code("x").await?;

    // Assert
    expect_that!(checked.status, eq(StatusCode::NOT_FOUND));
    expect_that!(token.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

/// Whether `response` is a refusal of the grant.
fn is_invalid_grant(response: &TestResponse) -> bool {
    response.status == StatusCode::BAD_REQUEST && response.body["error"] == json!("invalid_grant")
}

/// Expects the access and refresh tokens of `tokens`, a token response, to be dead.
async fn expect_dead(app: &TestApp, tokens: &TestResponse) -> Result<()> {
    let me = app
        .send_bearer(Method::GET, "/api/v1/me", &text(&tokens.body["access_token"]), None)
        .await?;
    let refreshed = refresh(app, &text(&tokens.body["refresh_token"])).await?;
    expect_that!(me.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(is_invalid_grant(&refreshed), eq(true), "{}", refreshed.body);
    Ok(())
}

// The races below run for real on MySQL and Postgres. In-memory SQLite has one connection, so there the two requests
// take turns, which is the order either race can end in. The assertions hold whichever side wins.

#[gtest]
#[tokio::test]
async fn of_two_exchanges_of_one_code_at_most_one_wins_and_its_tokens_end() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let code = app.approve_cli(&session).await?;

    // Act
    let (first, second) = tokio::join!(app.exchange_code(&code), app.exchange_code(&code));
    let (first, second) = (first?, second?);

    // Assert: the loser presented a used code within its lifetime, so the winner's family is revoked too.
    let (winner, loser) = if first.status == StatusCode::OK {
        (first, second)
    } else {
        (second, first)
    };
    expect_that!(winner.status, eq(StatusCode::OK), "{}", winner.body);
    expect_that!(is_invalid_grant(&loser), eq(true), "{}", loser.body);
    expect_dead(&app, &winner).await
}

#[gtest]
#[tokio::test]
async fn of_two_refreshes_with_one_token_at_most_one_wins_and_every_token_ends() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;

    // Act
    let (first, second) = tokio::join!(refresh(&app, &refresh_token), refresh(&app, &refresh_token));
    let (first, second) = (first?, second?);
    let me = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;

    // Assert
    let (winner, loser) = if first.status == StatusCode::OK {
        (first, second)
    } else {
        (second, first)
    };
    expect_that!(winner.status, eq(StatusCode::OK), "{}", winner.body);
    expect_that!(is_invalid_grant(&loser), eq(true), "{}", loser.body);
    expect_that!(me.status, eq(StatusCode::UNAUTHORIZED));
    expect_dead(&app, &winner).await
}

#[gtest]
#[tokio::test]
async fn a_revocation_racing_a_refresh_ends_the_new_pair_too() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (access, refresh_token) = app.sign_in_cli(&session).await?;

    // Act
    let (refreshed, revoked) = tokio::join!(refresh(&app, &refresh_token), revoke(&app, &refresh_token));
    let (refreshed, revoked) = (refreshed?, revoked?);
    let me = app.send_bearer(Method::GET, "/api/v1/me", &access, None).await?;

    // Assert: whichever went first, nothing of the family works afterwards.
    expect_that!(revoked.status, eq(StatusCode::OK));
    expect_that!(me.status, eq(StatusCode::UNAUTHORIZED));
    if refreshed.status == StatusCode::OK {
        expect_dead(&app, &refreshed).await?;
    } else {
        expect_that!(is_invalid_grant(&refreshed), eq(true), "{}", refreshed.body);
    }
    Ok(())
}
