//! Registration, sign-in and sign-out for local accounts. Sessions are carried in an `HttpOnly` cookie.

mod oidc;

use api_types::auth::{RegisterRequest, SignInRequest};
use api_types::me::UserDto;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use service::auth::{Registration, SessionGrant};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiJson, clear_session_cookie, session_cookie, session_token};
use crate::{ApiError, AppState, CookieSettings};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register))
        .routes(routes!(sign_in))
        .routes(routes!(sign_out))
        .merge(oidc::routes())
}

/// Creates an account with a personal team and heap, and signs in.
#[utoipa::path(post, path = "/auth/register", tag = "auth", request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registered and signed in; sets the session cookie", body = UserDto),
        (status = 409, description = "`email_taken` (`field` `email`), or `handle_taken` (`field` `username`)", body = Problem),
        (status = 422, description = "Invalid email, name or password, or `invalid_handle` with a `hint`", body = Problem),
    ))]
async fn register(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RegisterRequest>,
) -> Result<Response, ApiError> {
    let registration = Registration {
        email: &body.email,
        username: &body.username,
        display_name: &body.display_name,
        password: &body.password,
    };
    let cookies = &state.cookies;
    let grant = state.services.register(registration).await?;
    Ok(signed_in(StatusCode::CREATED, &grant, cookies))
}

/// Signs in with an email address or a username, and a password.
///
/// Each account gets a few attempts per time window, counted together whether it's named by email address or by
/// username, and so does each address or username without an account. Past that, attempts fail with `429` until
/// the window ends, and a successful sign-in starts the count over.
#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = SignInRequest,
    responses(
        (status = 200, description = "Signed in; sets the session cookie", body = UserDto),
        (status = 401, description = "`invalid_credentials`", body = Problem),
        (status = 422, description = "`field_empty` (`field` `login` or `password`); not counted as an attempt", body = Problem),
        (status = 429, description = "`too_many_attempts`; `Retry-After` gives the seconds to wait", body = Problem,
            headers(("Retry-After" = i64, description = "Seconds until the account may try again"))),
    ))]
async fn sign_in(State(state): State<AppState>, ApiJson(body): ApiJson<SignInRequest>) -> Result<Response, ApiError> {
    let cookies = &state.cookies;
    let grant = state.services.sign_in(&body.login, &body.password).await?;
    Ok(signed_in(StatusCode::OK, &grant, cookies))
}

/// Ends the current session. Succeeds when there is none.
///
/// Only the session cookie is ended. A bearer token is left alone, and the answer is still `204`: a client signs out
/// with `POST /oauth/revoke`.
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204, description = "Signed out")))]
async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let cookies = &state.cookies;
    if let Some(token) = session_token(&headers) {
        state.services.sign_out(&token).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_session_cookie(cookies))],
    )
        .into_response())
}

fn signed_in(status: StatusCode, grant: &SessionGrant, cookies: &CookieSettings) -> Response {
    signed_in_with(status, grant, cookies, UserDto::from(&grant.user))
}

/// `body` with the session cookie of `grant`.
pub(crate) fn signed_in_with(
    status: StatusCode,
    grant: &SessionGrant,
    cookies: &CookieSettings,
    body: impl Serialize,
) -> Response {
    let body = axum::Json(body);
    match session_cookie(&grant.token, cookies) {
        Some(cookie) => (status, [(header::SET_COOKIE, cookie)], body).into_response(),
        // Tokens are base64url, which is always a valid header value; a failure here is a bug.
        None => ApiError::Service(service::ServiceError::Unauthenticated).into_response(),
    }
}
