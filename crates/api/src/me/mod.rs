//! The signed-in user, their teams and heaps.

use axum::Json;
use axum::extract::State;
use domain::user::{DisplayName, User};
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::me::{MeView, TeamView};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{HeapSummaryDto, IfMatchHeader, RoleDto};
use crate::error::Problem;
use crate::extract::{ApiJson, CurrentUser, IfMatch};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_me, update_me))
}

#[derive(Serialize, ToSchema)]
#[schema(as = User)]
pub(crate) struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    /// Send back in `If-Match` when changing the display name.
    pub version: u32,
}

impl From<&User> for UserDto {
    fn from(user: &User) -> Self {
        Self {
            id: *user.id.as_uuid(),
            email: user.email.as_str().to_owned(),
            display_name: user.display_name.as_str().to_owned(),
            version: user.version.get(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = TeamSummary)]
pub(crate) struct TeamDto {
    pub id: Uuid,
    pub name: String,
    /// The caller's role in this team.
    pub role: RoleDto,
    pub heaps: Vec<HeapSummaryDto>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = Me)]
pub(crate) struct MeDto {
    pub user: UserDto,
    pub teams: Vec<TeamDto>,
}

impl From<MeView> for MeDto {
    fn from(me: MeView) -> Self {
        let team = |view: TeamView| TeamDto {
            id: *view.team.id.as_uuid(),
            name: view.team.name.as_str().to_owned(),
            role: view.role.into(),
            heaps: view.heaps.iter().map(HeapSummaryDto::from).collect(),
        };
        Self {
            user: UserDto::from(&me.user),
            teams: me.teams.into_iter().map(team).collect(),
        }
    }
}

/// The signed-in user with their teams and heaps.
#[utoipa::path(get, path = "/me", tag = "me", security(("session" = [])),
    responses((status = 200, body = MeDto), (status = 401, description = "`unauthenticated`", body = Problem)))]
async fn get_me(State(state): State<AppState>, CurrentUser(principal): CurrentUser) -> Result<Json<MeDto>, ApiError> {
    Ok(Json(state.services.load_me(principal).await?.into()))
}

#[derive(Deserialize, ToSchema)]
#[schema(as = MePatchRequest)]
pub(crate) struct MePatchDto {
    /// 1 to 100 characters.
    pub display_name: String,
}

/// Changes the signed-in user's display name. Send the user's `version` in `If-Match`.
#[utoipa::path(patch, path = "/me", tag = "me", security(("session" = [])),
    params(IfMatchHeader), request_body = MePatchDto,
    responses(
        (status = 200, body = UserDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 409, description = "`version_conflict`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_me(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<MePatchDto>,
) -> Result<Json<UserDto>, ApiError> {
    let display_name = DisplayName::parse(&body.display_name).map_err(ServiceError::from)?;
    let user = state
        .services
        .update_display_name(principal, display_name, expected)
        .await?;
    Ok(Json(UserDto::from(&user)))
}
