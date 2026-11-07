//! Teams: reading them and adding heaps to them. Changing teams and their members is in `manage`, which needs
//! accounts.

#[cfg(feature = "accounts")]
mod manage;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::TeamId;
use domain::permission::{MemberAction, TeamAction};
use domain::text::Name;
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::teams::{MemberView, TeamDetail};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::RoleDto;
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, CurrentUser};
use crate::heaps::HeapDto;
use crate::{ApiError, AppState, Authentication};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    // `GET` and `PATCH` share a path, so they are registered together.
    #[cfg(feature = "accounts")]
    let routes = OpenApiRouter::new()
        .routes(routes!(get_team, manage::update_team))
        .merge(manage::routes());
    #[cfg(not(feature = "accounts"))]
    let routes = OpenApiRouter::new().routes(routes!(get_team));
    routes.routes(routes!(create_heap))
}

/// An action the caller can take on a team.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TeamAction)]
pub(crate) enum TeamActionDto {
    Rename,
    AddMember,
    CreateHeap,
    Leave,
}

impl From<TeamAction> for TeamActionDto {
    fn from(action: TeamAction) -> Self {
        match action {
            TeamAction::Rename => Self::Rename,
            TeamAction::AddMember => Self::AddMember,
            TeamAction::CreateHeap => Self::CreateHeap,
            TeamAction::Leave => Self::Leave,
        }
    }
}

/// An action the caller can take on one member.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = MemberAction)]
pub(crate) enum MemberActionDto {
    Promote,
    Demote,
    Remove,
}

impl From<MemberAction> for MemberActionDto {
    fn from(action: MemberAction) -> Self {
        match action {
            MemberAction::Promote => Self::Promote,
            MemberAction::Demote => Self::Demote,
            MemberAction::Remove => Self::Remove,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = TeamMember)]
pub(crate) struct MemberDto {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: RoleDto,
    /// UTC milliseconds.
    pub joined_at: i64,
    pub allowed_actions: Vec<MemberActionDto>,
}

impl From<MemberView> for MemberDto {
    fn from(view: MemberView) -> Self {
        Self {
            user_id: *view.user.id.as_uuid(),
            display_name: view.user.display_name.as_str().to_owned(),
            email: view.user.email.as_str().to_owned(),
            role: view.membership.role.into(),
            joined_at: view.membership.joined_at.as_millis(),
            allowed_actions: view.allowed_actions.into_iter().map(MemberActionDto::from).collect(),
        }
    }
}

/// A team with its members and heaps, as the caller sees it.
#[derive(Serialize, ToSchema)]
#[schema(as = Team)]
pub(crate) struct TeamDto {
    pub id: Uuid,
    pub name: String,
    /// Send back in `If-Match` when renaming.
    pub version: u32,
    /// The caller's role.
    pub role: RoleDto,
    /// In the order they joined.
    pub members: Vec<MemberDto>,
    /// By name, archived ones included.
    pub heaps: Vec<HeapDto>,
    pub allowed_actions: Vec<TeamActionDto>,
}

impl TeamDto {
    /// The team as `authentication`'s caller sees it. The local user has no one to add and only the one team, so
    /// creating heaps is the only team action offered, and no member actions are.
    pub(crate) fn new(mut detail: TeamDetail, authentication: &Authentication) -> Self {
        if authentication.is_single_user() {
            detail
                .allowed_actions
                .retain(|action| matches!(action, TeamAction::CreateHeap));
            for member in &mut detail.members {
                member.allowed_actions.clear();
            }
        }
        Self {
            id: *detail.team.id.as_uuid(),
            name: detail.team.name.as_str().to_owned(),
            version: detail.team.version.get(),
            role: detail.role.into(),
            members: detail.members.into_iter().map(MemberDto::from).collect(),
            heaps: detail.heaps.into_iter().map(HeapDto::from).collect(),
            allowed_actions: detail.allowed_actions.into_iter().map(TeamActionDto::from).collect(),
        }
    }
}

/// A team or heap name.
#[derive(Deserialize, ToSchema)]
#[schema(as = NameRequest)]
pub(crate) struct NameDto {
    /// 1 to 100 characters.
    pub name: String,
}

impl NameDto {
    pub(crate) fn to_name(&self) -> Result<Name, ServiceError> {
        Ok(Name::parse(&self.name)?)
    }
}

/// The team with its members and heaps.
#[utoipa::path(get, path = "/teams/{team_id}", tag = "teams", security(("session" = [])),
    params(("team_id" = Uuid, Path)),
    responses((status = 200, body = TeamDto), (status = 404, body = Problem)))]
async fn get_team(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(team_id): ApiPath<Uuid>,
) -> Result<Json<TeamDto>, ApiError> {
    let detail = state.services.load_team(principal, TeamId::from_uuid(team_id)).await?;
    Ok(Json(TeamDto::new(detail, &state.authentication)))
}

/// Adds a heap to the team. Admins only.
#[utoipa::path(post, path = "/teams/{team_id}/heaps", tag = "heaps", security(("session" = [])),
    params(("team_id" = Uuid, Path)), request_body = NameDto,
    responses(
        (status = 201, body = HeapDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 422, body = Problem),
    ))]
async fn create_heap(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(team_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<NameDto>,
) -> Result<(StatusCode, Json<HeapDto>), ApiError> {
    let view = state
        .services
        .create_heap(principal, TeamId::from_uuid(team_id), body.to_name()?)
        .await?;
    Ok((StatusCode::CREATED, Json(view.into())))
}
