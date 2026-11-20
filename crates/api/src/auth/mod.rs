//! Registration, sign-in and sign-out for local accounts. Sessions are carried in an `HttpOnly` cookie.

mod oidc;

use api_types::auth::{ConfirmationSentDto, RegisterRequest, RegistrationOptionsDto, SignInRequest, SignedOut};
use api_types::me::UserDto;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use domain::user::User;
use serde::Serialize;
use service::auth::{Registration, SessionGrant};
use service::email_confirmation::Registered;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiJson, clear_session_cookie, session_cookie, session_token};
use crate::{ApiError, AppState, CookieSettings};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(registration_options))
        .routes(routes!(register))
        .routes(routes!(sign_in))
        .routes(routes!(sign_out))
        .merge(oidc::routes())
}

/// Whether the server lets anyone create an account. Needs no sign-in.
///
/// The sign-in page uses it to decide whether to offer Create account. An invite link still creates an account on a
/// server where registration is closed.
#[utoipa::path(get, path = "/auth/registration", tag = "auth",
    responses((status = 200, description = "Whether registration is open", body = RegistrationOptionsDto)))]
async fn registration_options(State(state): State<AppState>) -> Json<RegistrationOptionsDto> {
    Json(RegistrationOptionsDto {
        open: state.services.registration_open(),
    })
}

/// Creates an account with a personal team and heap, and signs in. On a server with mail set up it doesn't sign in:
/// the account can't sign in until it opens the confirmation link mailed to its address.
#[utoipa::path(post, path = "/auth/register", tag = "auth", request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registered and signed in; sets the session cookie", body = UserDto),
        (status = 202, description = "With mail set up: made, and a confirmation link is mailed; not signed in", body = ConfirmationSentDto),
        (status = 403, description = "`password_sign_in_off`: the server signs in through its provider only", body = Problem),
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
    Ok(match state.services.register(registration).await? {
        Registered::SignedIn(grant) => signed_in(StatusCode::CREATED, &grant, cookies),
        Registered::ConfirmationSent(user) => confirmation_sent(&user),
    })
}

/// Signs in with an email address or a username, and a password.
///
/// Each account gets a few attempts per time window, counted together whether it's named by email address or by
/// username, and so does each address or username without an account. Past that, attempts fail with `429` until
/// the window ends, and a successful sign-in starts the count over.
///
/// On a server with mail set up, the right password for an account that hasn't confirmed its address doesn't sign in:
/// a new confirmation link is mailed, cancelling the earlier one, and the answer is `403`. The attempt isn't cleared,
/// so the mails are limited like sign-ins.
#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = SignInRequest,
    responses(
        (status = 200, description = "Signed in; sets the session cookie", body = UserDto),
        (status = 401, description = "`invalid_credentials`", body = Problem),
        (status = 403, description = "`password_sign_in_off`: the server signs in through its provider only; or `email_not_confirmed`: the account hasn't confirmed its address, and a new link is mailed", body = Problem),
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
///
/// When the server is set to sign people out at their OpenID Connect provider too, the session came from the
/// provider and the provider supports it, the answer is `200` with the URL to send the browser to. The session is
/// ended either way, and a provider that can't be reached gives `204`.
#[utoipa::path(post, path = "/auth/logout", tag = "auth",
    responses(
        (status = 204, description = "Signed out"),
        (status = 200, description = "Signed out; the browser goes on to the provider to end its session there", body = SignedOut),
    ))]
async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let cookies = &state.cookies;
    let provider_logout_url = match session_token(&headers) {
        Some(token) => state.services.sign_out(&token).await?,
        None => None,
    };
    let cleared = [(header::SET_COOKIE, clear_session_cookie(cookies))];
    Ok(match provider_logout_url {
        Some(provider_logout_url) => {
            (StatusCode::OK, cleared, axum::Json(SignedOut { provider_logout_url })).into_response()
        }
        None => (StatusCode::NO_CONTENT, cleared).into_response(),
    })
}

fn signed_in(status: StatusCode, grant: &SessionGrant, cookies: &CookieSettings) -> Response {
    signed_in_with(status, grant, cookies, UserDto::from(&grant.user))
}

/// The answer for an account made on a server with mail set up: no session, and where the link went.
pub(crate) fn confirmation_sent(user: &User) -> Response {
    let body = ConfirmationSentDto {
        email: user.email.as_str().to_owned(),
    };
    (StatusCode::ACCEPTED, axum::Json(body)).into_response()
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
