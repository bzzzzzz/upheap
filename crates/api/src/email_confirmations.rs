//! Confirming an address by the link mailed to a new account, by the link's token. Registering and signing in send
//! the link (`POST /auth/register`, `POST /auth/login`).

use api_types::me::UserDto;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use domain::password_reset::ResetToken;
use serde::Deserialize;
use service::ServiceError;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::signed_in_with;
use crate::error::Problem;
use crate::extract::ApiPath;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(confirm_email))
}

/// `{token}`, for documenting the path as well as reading it.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ConfirmationTokenParams {
    /// The link's secret: the last part of the link, `{origin}/confirm/{token}`.
    token: String,
}

/// Confirms the account's email address with the link mailed to it, and signs this browser in as the account. The
/// link works once, for a day, and a newer one for the account cancels it. Anyone holding the token may; a session
/// cookie or bearer token is ignored, and the answer's cookie replaces any session cookie the browser had. There is
/// no read of a link: confirming is a `POST`, so a mail scanner that opens the link confirms nothing.
///
/// Works whether the server has mail set up or not. Not rate limited, as a token can't be guessed.
#[utoipa::path(post, path = "/email-confirmations/{token}", tag = "auth",
    params(ConfirmationTokenParams),
    responses(
        (status = 200, description = "Confirmed; the answer is the account, and it sets the session cookie", body = UserDto),
        (status = 403, description = "`csrf_header_missing`, `password_sign_in_off`, or `account_deactivated`", body = Problem),
        (status = 404, description = "No such link", body = Problem),
        (status = 409, description = "`confirmation_link_used`, `confirmation_link_cancelled` or `confirmation_link_ended`", body = Problem),
    ))]
async fn confirm_email(
    State(state): State<AppState>,
    path: ApiPath<ConfirmationTokenParams>,
) -> Result<Response, ApiError> {
    let token = ResetToken::parse(&path.0.token).ok_or(ServiceError::NotFound)?;
    let cookies = &state.cookies;
    let grant = state.services.confirm_email(&token).await?;
    Ok(signed_in_with(
        StatusCode::OK,
        &grant,
        cookies,
        UserDto::from(&grant.user),
    ))
}
