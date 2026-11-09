//! Registration, sign-in and sign-out for local accounts. Sessions are carried in an `HttpOnly` cookie.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use service::auth::{Registration, SessionGrant};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiJson, clear_session_cookie, session_cookie, session_token};
use crate::me::UserDto;
use crate::{ApiError, AppState, Authentication, CookieSettings};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register))
        .routes(routes!(sign_in))
        .routes(routes!(sign_out))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct RegisterRequest {
    pub email: String,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Becomes the slug of
    /// the personal team, and can't be changed later.
    #[schema(example = "eugene")]
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SignInRequest {
    /// An email address, or a username. `email` is still accepted in its place.
    #[serde(alias = "email")]
    pub login: String,
    pub password: String,
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
    let cookies = cookie_settings(&state)?;
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
    let cookies = cookie_settings(&state)?;
    let grant = state.services.sign_in(&body.login, &body.password).await?;
    Ok(signed_in(StatusCode::OK, &grant, cookies))
}

/// Ends the current session. Succeeds when there is none.
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204, description = "Signed out")))]
async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let cookies = cookie_settings(&state)?;
    if let Some(token) = session_token(&headers) {
        state.services.sign_out(&token).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_session_cookie(cookies))],
    )
        .into_response())
}

/// How to set the session cookie. A server whose one user needs no signing in has no such routes.
fn cookie_settings(state: &AppState) -> Result<&CookieSettings, ApiError> {
    match &state.authentication {
        Authentication::Sessions(cookies) => Ok(cookies),
        Authentication::LocalUser(_) => Err(ApiError::NoSuchRoute),
    }
}

fn signed_in(status: StatusCode, grant: &SessionGrant, cookies: &CookieSettings) -> Response {
    let body = axum::Json(UserDto::from(&grant.user));
    match session_cookie(&grant.token, cookies) {
        Some(cookie) => (status, [(header::SET_COOKIE, cookie)], body).into_response(),
        // Tokens are base64url, which is always a valid header value; a failure here is a bug.
        None => ApiError::Service(service::ServiceError::Unauthenticated).into_response(),
    }
}
