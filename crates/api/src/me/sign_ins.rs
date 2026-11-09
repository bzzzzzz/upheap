//! The signed-in user's sign-ins by clients such as the CLI: listing them and ending one.

use api_types::me::{ClientSignInDto, ClientSignInListDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::TokenFamilyId;
use serde::Deserialize;
use service::oauth::ClientSignIn;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::Problem;
use crate::extract::{ApiPath, CurrentUser};
use crate::oauth::require_accounts;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_sign_ins, end_sign_in))
}

/// `{id}` in the sign-in's path.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
struct SignInParams {
    /// The sign-in's `id`, from the list.
    id: Uuid,
}

fn to_sign_in_dto(sign_in: ClientSignIn) -> ClientSignInDto {
    ClientSignInDto {
        id: *sign_in.id.as_uuid(),
        client_id: sign_in.client_id.to_owned(),
        client_name: sign_in.client_name.to_owned(),
        device_name: sign_in.device_name,
        signed_in_at: sign_in.signed_in_at.as_millis(),
        last_used_at: sign_in.last_used_at.as_millis(),
    }
}

/// The caller's live sign-ins by clients such as the CLI, the most recently used first. A sign-in is live while its
/// refresh token can still be used; ended and expired ones aren't listed.
#[utoipa::path(get, path = "/me/sign-ins", tag = "me", security(("session" = [])),
    responses(
        (status = 200, body = ClientSignInListDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
    ))]
async fn list_sign_ins(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
) -> Result<Json<ClientSignInListDto>, ApiError> {
    require_accounts(&state)?;
    let sign_ins = state.services.list_sign_ins(principal).await?;
    Ok(Json(ClientSignInListDto {
        sign_ins: sign_ins.into_iter().map(to_sign_in_dto).collect(),
    }))
}

/// Ends one of the caller's sign-ins: its access and refresh tokens stop working at once. Succeeds when it was
/// ended already. A bearer token may end its own sign-in; its next request is then refused as signed out.
#[utoipa::path(delete, path = "/me/sign-ins/{id}", tag = "me", security(("session" = [])),
    params(SignInParams),
    responses(
        (status = 204, description = "Ended, or ended already"),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 404, description = "No such sign-in, or it isn't the caller's", body = Problem),
    ))]
async fn end_sign_in(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(SignInParams { id }): ApiPath<SignInParams>,
) -> Result<StatusCode, ApiError> {
    require_accounts(&state)?;
    state
        .services
        .end_sign_in(principal, TokenFamilyId::from_uuid(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
