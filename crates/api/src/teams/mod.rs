//! Teams: creating and renaming them, their members and roles, and adding heaps to them.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::permission::{MemberAction, TeamAction};
use domain::text::Name;
use domain::{TeamId, UserId};
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::teams::{MemberAdded, MemberView, TeamDetail};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{IfMatchHeader, RoleDto};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, CurrentUser, IfMatch};
use crate::heaps::HeapDto;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_team))
        .routes(routes!(get_team, update_team))
        .routes(routes!(add_member))
        .routes(routes!(update_member, remove_member))
        .routes(routes!(leave_team))
        .routes(routes!(create_heap))
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

impl From<TeamDetail> for TeamDto {
    fn from(detail: TeamDetail) -> Self {
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

#[derive(Deserialize, ToSchema)]
#[schema(as = AddMemberRequest)]
pub(crate) struct AddMemberDto {
    /// From `GET /users`. New members join as `member`.
    pub user_id: Uuid,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = MemberPatchRequest)]
pub(crate) struct MemberPatchDto {
    pub role: RoleDto,
}

/// Creates a team with the caller as its admin. The team starts without heaps.
#[utoipa::path(post, path = "/teams", tag = "teams", security(("session" = [])), request_body = NameDto,
    responses((status = 201, body = TeamDto), (status = 422, body = Problem)))]
async fn create_team(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiJson(body): ApiJson<NameDto>,
) -> Result<(StatusCode, Json<TeamDto>), ApiError> {
    let detail = state.services.create_team(principal, body.to_name()?).await?;
    Ok((StatusCode::CREATED, Json(detail.into())))
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
    Ok(Json(detail.into()))
}

/// Renames the team. Admins only; send the team's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team_id}", tag = "teams", security(("session" = [])),
    params(("team_id" = Uuid, Path), IfMatchHeader), request_body = NameDto,
    responses(
        (status = 200, body = TeamDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_team(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiPath(team_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<NameDto>,
) -> Result<Json<TeamDto>, ApiError> {
    let detail = state
        .services
        .rename_team(principal, TeamId::from_uuid(team_id), body.to_name()?, expected)
        .await?;
    Ok(Json(detail.into()))
}

/// Adds an existing user to the team as a member. Admins only. Answers `200` with the team unchanged when they are
/// already a member.
#[utoipa::path(post, path = "/teams/{team_id}/members", tag = "teams", security(("session" = [])),
    params(("team_id" = Uuid, Path)), request_body = AddMemberDto,
    responses(
        (status = 201, body = TeamDto),
        (status = 200, description = "Already a member", body = TeamDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 422, description = "`user_not_found`", body = Problem),
    ))]
async fn add_member(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(team_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AddMemberDto>,
) -> Result<(StatusCode, Json<TeamDto>), ApiError> {
    let (detail, outcome) = state
        .services
        .add_member(principal, TeamId::from_uuid(team_id), UserId::from_uuid(body.user_id))
        .await?;
    let status = match outcome {
        MemberAdded::Added => StatusCode::CREATED,
        MemberAdded::AlreadyMember => StatusCode::OK,
    };
    Ok((status, Json(detail.into())))
}

/// Makes a member an admin, or an admin a member. Admins only.
#[utoipa::path(patch, path = "/teams/{team_id}/members/{user_id}", tag = "teams", security(("session" = [])),
    params(("team_id" = Uuid, Path), ("user_id" = Uuid, Path)), request_body = MemberPatchDto,
    responses(
        (status = 200, body = TeamDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`last_admin`", body = Problem),
    ))]
async fn update_member(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath((team_id, user_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<MemberPatchDto>,
) -> Result<Json<TeamDto>, ApiError> {
    let detail = state
        .services
        .change_member_role(
            principal,
            TeamId::from_uuid(team_id),
            UserId::from_uuid(user_id),
            body.role.into(),
        )
        .await?;
    Ok(Json(detail.into()))
}

/// Takes a user out of the team. Admins only. Removing someone who isn't a member succeeds.
#[utoipa::path(delete, path = "/teams/{team_id}/members/{user_id}", tag = "teams", security(("session" = [])),
    params(("team_id" = Uuid, Path), ("user_id" = Uuid, Path)),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`last_admin`", body = Problem),
    ))]
async fn remove_member(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath((team_id, user_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .remove_member(principal, TeamId::from_uuid(team_id), UserId::from_uuid(user_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Leaves the team. The last admin can't leave until another member is an admin.
#[utoipa::path(post, path = "/teams/{team_id}/leave", tag = "teams", security(("session" = [])),
    params(("team_id" = Uuid, Path)),
    responses(
        (status = 204, description = "Left the team"),
        (status = 404, body = Problem),
        (status = 409, description = "`last_admin`", body = Problem),
    ))]
async fn leave_team(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(team_id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.services.leave_team(principal, TeamId::from_uuid(team_id)).await?;
    Ok(StatusCode::NO_CONTENT)
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
