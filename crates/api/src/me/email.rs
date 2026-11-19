//! The signed-in user's email address: asking to change it from the browser.

use api_types::me::RequestEmailChangeDto;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiJson, SessionUser, session_token};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(request_email_change))
}

/// Asks to change the caller's email address: a link is mailed to the new address, and the address changes when the
/// link is applied (`POST /email-changes/{token}`). Takes the session cookie only, never a bearer token: it is the
/// person's decision in their browser, as a password change is.
///
/// Answers `202` with no body whether or not another account has the new address, so it reveals nothing about
/// accounts; the mail is sent after the answer. The link works once, for a day, and a newer ask cancels it. The
/// caller's own current address is treated like a free one.
///
/// Tries are limited per session, with the same limit as sign-in: past it the session gets `429` before the password
/// is checked. Unlike a password change, a success doesn't start the count over, so the limit, five in 15 minutes,
/// also limits the mails a session sends. A wrong current password also adds one to the account's sign-in attempts.
#[utoipa::path(post, path = "/me/email-changes", tag = "me", security(("session" = [])),
    request_body = RequestEmailChangeDto,
    responses(
        (status = 202, description = "Accepted; if the address is free, a link is on its way to it"),
        (status = 401, description = "`unauthenticated`: no session cookie, or the request carried a bearer token", body = Problem),
        (status = 403, description = "`csrf_header_missing`", body = Problem),
        (status = 404, description = "`not_found`: the server has no mail set up", body = Problem),
        (status = 422, description = "`wrong_password` (`field` `current_password`); `field_empty` (`field` `current_password` or `email`); `email_invalid` (`field` `email`). Not counted as an attempt, except `wrong_password`", body = Problem),
        (status = 429, description = "`too_many_attempts`; `Retry-After` gives the seconds to wait", body = Problem,
            headers(("Retry-After" = i64, description = "Seconds until the session may try again"))),
    ))]
async fn request_email_change(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    headers: HeaderMap,
    ApiJson(body): ApiJson<RequestEmailChangeDto>,
) -> Result<StatusCode, ApiError> {
    // `SessionUser` found this cookie valid, so a missing one only means the session ended in between.
    let session = session_token(&headers).ok_or(ServiceError::Unauthenticated)?;
    state
        .services
        .request_email_change(principal, &session, &body.email, &body.current_password)
        .await?;
    Ok(StatusCode::ACCEPTED)
}
