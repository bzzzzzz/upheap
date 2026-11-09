//! The signed-in user, their teams and heaps.

use axum::Json;
use axum::extract::State;
use domain::user::{DisplayName, User};
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::me::{HeapOverview, MeView, TeamView};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{HeapSummaryDto, IfMatchHeader, RoleDto};
use crate::error::Problem;
use crate::extract::{ApiJson, CurrentUser, IfMatch};
use crate::{ApiError, AppState, Authentication};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_me, update_me))
}

#[derive(Serialize, ToSchema)]
#[schema(as = User)]
pub(crate) struct UserDto {
    pub id: Uuid,
    /// Also the slug of the user's personal team. Never changes.
    pub username: String,
    pub email: String,
    pub display_name: String,
    /// Send back in `If-Match` when changing the display name.
    pub version: u32,
}

impl From<&User> for UserDto {
    fn from(user: &User) -> Self {
        Self {
            id: *user.id.as_uuid(),
            username: user.username.to_string(),
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
    /// The team's path is `/teams/{slug}`.
    pub slug: String,
    pub name: String,
    /// Whether this is the caller's personal team, which is theirs alone.
    pub personal: bool,
    /// The caller's role in this team.
    pub role: RoleDto,
    /// Everyone in the team, the caller included.
    pub member_count: u32,
    /// By name, archived ones included.
    pub heaps: Vec<HeapOverviewDto>,
}

/// A heap of one of the caller's teams, with how many items it has open.
#[derive(Serialize, ToSchema)]
#[schema(as = HeapOverview)]
pub(crate) struct HeapOverviewDto {
    #[serde(flatten)]
    pub heap: HeapSummaryDto,
    /// Items the heap's list shows by default, before filters: open and in progress ones, as the list's
    /// `unfiltered_count` with the default view.
    pub open_count: u32,
}

impl From<&HeapOverview> for HeapOverviewDto {
    fn from(overview: &HeapOverview) -> Self {
        Self {
            heap: HeapSummaryDto::from(&overview.heap),
            open_count: overview.open_count,
        }
    }
}

/// An action the caller can take on their account.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = MeAction)]
pub(crate) enum MeActionDto {
    /// End the session.
    SignOut,
    /// Start a new team.
    CreateTeam,
}

#[derive(Serialize, ToSchema)]
#[schema(as = Me)]
pub(crate) struct MeDto {
    pub user: UserDto,
    pub teams: Vec<TeamDto>,
    /// Empty for the local build's one user, who can't sign out and has only the one team.
    pub allowed_actions: Vec<MeActionDto>,
}

impl MeDto {
    fn new(me: MeView, authentication: &Authentication) -> Self {
        let allowed_actions = if authentication.is_single_user() {
            Vec::new()
        } else {
            vec![MeActionDto::SignOut, MeActionDto::CreateTeam]
        };
        let team = |view: TeamView| TeamDto {
            id: *view.team.id.as_uuid(),
            slug: view.team.slug.to_string(),
            name: view.team.name.as_str().to_owned(),
            personal: view.team.is_personal(),
            role: view.role.into(),
            member_count: view.member_count,
            heaps: view.heaps.iter().map(HeapOverviewDto::from).collect(),
        };
        Self {
            user: UserDto::from(&me.user),
            teams: me.teams.into_iter().map(team).collect(),
            allowed_actions,
        }
    }
}

/// The signed-in user with their teams and heaps.
#[utoipa::path(get, path = "/me", tag = "me", security(("session" = [])),
    responses((status = 200, body = MeDto), (status = 401, description = "`unauthenticated`", body = Problem)))]
async fn get_me(State(state): State<AppState>, CurrentUser(principal): CurrentUser) -> Result<Json<MeDto>, ApiError> {
    Ok(Json(MeDto::new(
        state.services.load_me(principal).await?,
        &state.authentication,
    )))
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
