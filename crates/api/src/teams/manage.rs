//! Changing teams: creating and renaming them, and their members and roles. The local build's one team is made with
//! its user and nobody else can join, so it leaves these out (the `accounts` feature).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::{TeamId, UserId};
use serde::Deserialize;
use service::teams::MemberAdded;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{NameDto, TeamDto};
use crate::dto::{IfMatchHeader, RoleDto};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, CurrentUser, IfMatch};
use crate::{ApiError, AppState};

/// Every route but `PATCH /teams/{team_id}`, which `super` registers with the `GET` on its path.
pub(super) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_team))
        .routes(routes!(add_member))
        .routes(routes!(update_member, remove_member))
        .routes(routes!(leave_team))
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
    Ok((StatusCode::CREATED, Json(TeamDto::new(detail, &state.authentication))))
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
pub(super) async fn update_team(
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
    Ok(Json(TeamDto::new(detail, &state.authentication)))
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
    Ok((status, Json(TeamDto::new(detail, &state.authentication))))
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
    Ok(Json(TeamDto::new(detail, &state.authentication)))
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
