//! Opening a password reset link and setting a password through it, by the link's token. The operator makes links
//! with a server command (README, Resetting a password). The local build has no passwords, so it leaves these out
//! (the `accounts` feature).

use api_types::auth::{PasswordResetDto, PasswordResetStateDto, SetPasswordRequestDto};
use api_types::me::UserDto;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use domain::password_reset::ResetToken;
use serde::Deserialize;
use service::ServiceError;
use service::password_reset::ResetLinkView;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{cookie_settings, signed_in_with};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open_password_reset))
        .routes(routes!(reset_password))
}

/// `{token}`, for documenting the path as well as reading it.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ResetTokenParams {
    /// The reset link's secret: the last part of the link, `{origin}/reset/{token}`.
    token: String,
}

/// The token in the path. One that can't be a token names nothing, so it is `404` without reading the database.
fn parse_token(params: ResetTokenParams) -> Result<ResetToken, ServiceError> {
    ResetToken::parse(&params.token).ok_or(ServiceError::NotFound)
}

fn to_password_reset_dto(view: ResetLinkView) -> PasswordResetDto {
    PasswordResetDto {
        state: PasswordResetStateDto::from(view.state),
        display_name: view.user.display_name.as_str().to_owned(),
        username: view.user.username.to_string(),
    }
}

/// Opens a reset link: whose account it is for and whether it works, was used, was cancelled or has ended. Anyone
/// holding the token may; no sign-in is needed, and a session cookie or bearer token is ignored. A link is cancelled
/// when a newer one is made for the account, or when its password is changed while the link works.
#[utoipa::path(get, path = "/password-resets/{token}", tag = "auth",
    params(ResetTokenParams),
    responses(
        (status = 200, description = "The link, in whatever state it is in", body = PasswordResetDto),
        (status = 404, description = "No such link", body = Problem),
    ))]
async fn open_password_reset(
    State(state): State<AppState>,
    path: ApiPath<ResetTokenParams>,
) -> Result<Json<PasswordResetDto>, ApiError> {
    let token = parse_token(path.0)?;
    let view = state.services.open_password_reset(&token).await?;
    Ok(Json(to_password_reset_dto(view)))
}

/// Sets a new password for the link's account, and signs this browser in as it. The link works for 60 minutes after
/// the operator made it, and once: it is used up by this call.
///
/// Setting the password ends every sign-in of the account: its web sessions, in this browser or any other, and its
/// clients such as the command line, whose tokens stop working at once. Then this browser gets a new session, in the
/// cookie of the answer, which replaces any session cookie it had, even another account's. The new password may equal
/// the old one. The account's sign-in attempts start over.
///
/// The link is checked first, then the password: a link that doesn't work answers `409` whatever the password, and a
/// password that is refused leaves the link working. Not rate limited, as a token can't be guessed.
#[utoipa::path(post, path = "/password-resets/{token}", tag = "auth",
    params(ResetTokenParams), request_body = SetPasswordRequestDto,
    responses(
        (status = 200, description = "Set; the answer is the account, and it sets the session cookie", body = UserDto),
        (status = 403, description = "`csrf_header_missing`", body = Problem),
        (status = 404, description = "No such link", body = Problem),
        (status = 409, description = "`reset_link_used`, `reset_link_cancelled` or `reset_link_ended`", body = Problem),
        (status = 422, description = "`field_empty`, `password_too_short` or `password_too_long` (`field` `password`). The link keeps working", body = Problem),
    ))]
async fn reset_password(
    State(state): State<AppState>,
    path: ApiPath<ResetTokenParams>,
    ApiJson(body): ApiJson<SetPasswordRequestDto>,
) -> Result<Response, ApiError> {
    let token = parse_token(path.0)?;
    let cookies = cookie_settings(&state)?;
    let grant = state.services.reset_password(&token, &body.password).await?;
    Ok(signed_in_with(
        StatusCode::OK,
        &grant,
        cookies,
        UserDto::from(&grant.user),
    ))
}
