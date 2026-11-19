//! Applying an email change link, by the link's token. Asking for one is `POST /me/email-changes`.

use api_types::auth::EmailChangedDto;
use axum::Json;
use axum::extract::State;
use domain::password_reset::ResetToken;
use serde::Deserialize;
use service::ServiceError;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::ApiPath;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(apply_email_change))
}

/// `{token}`, for documenting the path as well as reading it.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct EmailChangeTokenParams {
    /// The link's secret: the last part of the link, `{origin}/change-email/{token}`.
    token: String,
}

/// Changes the account's email address to the one the link was mailed to, and mails a notice to the old address. The
/// link works once, for a day. Anyone holding the token may; no sign-in is needed, a session cookie or bearer token is
/// ignored, and it doesn't sign anyone in. There is no read of a link: applying is a `POST`, so a mail scanner that
/// opens the link changes nothing.
///
/// The account's sessions and tokens are left alone. Its working password reset link is cancelled, since one mailed
/// to the old address must not keep working. Not rate limited, as a token can't be guessed.
#[utoipa::path(post, path = "/email-changes/{token}", tag = "auth",
    params(EmailChangeTokenParams),
    responses(
        (status = 200, description = "Changed; the answer is the new address", body = EmailChangedDto),
        (status = 403, description = "`csrf_header_missing`", body = Problem),
        (status = 404, description = "No such link, or the server has no mail set up", body = Problem),
        (status = 409, description = "`email_change_used`, `email_change_cancelled` or `email_change_ended`; or `email_taken` (`field` `email`): another account has the address by now, and the link keeps working", body = Problem),
    ))]
async fn apply_email_change(
    State(state): State<AppState>,
    path: ApiPath<EmailChangeTokenParams>,
) -> Result<Json<EmailChangedDto>, ApiError> {
    let token = ResetToken::parse(&path.0.token).ok_or(ServiceError::NotFound)?;
    let email = state.services.apply_email_change(&token).await?;
    Ok(Json(EmailChangedDto {
        email: email.as_str().to_owned(),
    }))
}
