//! The signed-in user, their teams and heaps.

use axum::Json;
use axum::extract::State;
use domain::user::User;
use serde::Serialize;
use service::me::{MeView, TeamView};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{HeapSummaryDto, RoleDto};
use crate::error::Problem;
use crate::extract::CurrentUser;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_me))
}

#[derive(Serialize, ToSchema)]
#[schema(as = User)]
pub(crate) struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}

impl From<&User> for UserDto {
    fn from(user: &User) -> Self {
        Self {
            id: *user.id.as_uuid(),
            email: user.email.as_str().to_owned(),
            display_name: user.display_name.as_str().to_owned(),
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
