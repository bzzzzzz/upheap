//! The signed-in user's password: changing it from the browser.

use api_types::me::{ChangePasswordRequestDto, PasswordChangedDto};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::signed_in_with;
use crate::error::Problem;
use crate::extract::{ApiJson, SessionUser, session_token};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(change_password))
}

/// Changes the caller's password. Takes the session cookie only, never a bearer token: it is the person's decision
/// in their browser, and a client holding a token must not be able to lock the person out of the rest.
///
/// The session it is made from is replaced: the answer sets a new session cookie and the old one stops working, so
/// the browser stays signed in and a copy of its old cookie doesn't. Every other session of the user ends, and so
/// does every sign-in by a client such as the command line: their tokens stop working at once. `ended_sign_ins`
/// counts the other ones that were still live. The new password may equal the current one, which ends the others
/// without picking a new password.
///
/// Tries are limited per session, with the same limit as sign-in: past it the session gets `429` before the password
/// is checked. A wrong current password also adds one to the account's sign-in attempts, but the account's count
/// doesn't block a change. A success starts both counts over.
#[utoipa::path(post, path = "/me/password", tag = "me", security(("session" = [])),
    request_body = ChangePasswordRequestDto,
    responses(
        (status = 200, description = "Changed; sets a new session cookie, and the old one stops working", body = PasswordChangedDto),
        (status = 401, description = "`unauthenticated`: no session cookie, or the request carried a bearer token", body = Problem),
        (status = 403, description = "`csrf_header_missing`", body = Problem),
        (status = 422, description = "`wrong_password` (`field` `current_password`); `field_empty` (`field` `current_password` or `password`), `password_too_short` or `password_too_long` (`field` `password`). Not counted as an attempt, except `wrong_password`", body = Problem),
        (status = 429, description = "`too_many_attempts`; `Retry-After` gives the seconds to wait", body = Problem,
            headers(("Retry-After" = i64, description = "Seconds until the session may try again"))),
    ))]
async fn change_password(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ChangePasswordRequestDto>,
) -> Result<Response, ApiError> {
    let cookies = &state.cookies;
    // `SessionUser` found this cookie valid, so a missing one only means the session ended in between.
    let session = session_token(&headers).ok_or(ServiceError::Unauthenticated)?;
    let changed = state
        .services
        .change_password(principal, &session, &body.current_password, &body.password)
        .await?;
    let body = PasswordChangedDto {
        ended_sign_ins: changed.ended_sign_ins,
    };
    Ok(signed_in_with(StatusCode::OK, &changed.session, cookies, body))
}
