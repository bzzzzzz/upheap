//! Changing teams: creating and renaming them, and their members and roles.

use api_types::teams::{AddMemberDto, CreateNamedDto, MemberPatchDto, TeamDto, TeamPatchDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::UserId;
use domain::handle::Handle;
use domain::text::Name;
use domain::validation::Field;
use service::ServiceError;
use service::teams::{MemberAdded, TeamPatch};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::{to_name, to_slug, to_team_dto};
use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, CurrentUser, IfMatch};
use crate::paths::{MemberParams, MemberPath, TeamParams, TeamPath};
use crate::{ApiError, AppState};

/// Every route but `PATCH /teams/{team}`, which `super` registers with the `GET` on its path.
pub(super) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_team))
        .routes(routes!(add_member))
        .routes(routes!(update_member, remove_member))
        .routes(routes!(leave_team))
}

fn to_team_patch(body: &TeamPatchDto) -> Result<TeamPatch, ServiceError> {
    Ok(TeamPatch {
        name: body.name.as_deref().map(Name::parse).transpose()?,
        slug: body
            .slug
            .as_deref()
            .map(|raw| Handle::parse(raw, Field::Slug))
            .transpose()?,
    })
}

/// Creates a team with the caller as its admin. The team starts without heaps.
#[utoipa::path(post, path = "/teams", tag = "teams", security(("session" = [])), request_body = CreateNamedDto,
    responses(
        (status = 201, body = TeamDto),
        (status = 409, description = "`handle_taken` (`field` `slug`), by a team or a username", body = Problem),
        (status = 422, description = "Invalid fields, or `invalid_handle` with a `hint`", body = Problem),
    ))]
async fn create_team(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiJson(body): ApiJson<CreateNamedDto>,
) -> Result<(StatusCode, Json<TeamDto>), ApiError> {
    let detail = state
        .services
        .create_team(principal, to_name(&body)?, to_slug(&body)?)
        .await?;
    Ok((StatusCode::CREATED, Json(to_team_dto(detail))))
}

/// Renames the team or changes its slug. Admins only; send the team's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}", tag = "teams", security(("session" = [])),
    params(TeamParams, IfMatchHeader), request_body = TeamPatchDto,
    responses(
        (status = 200, body = TeamDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `handle_taken` (`field` `slug`) or `personal_team`", body = Problem),
        (status = 422, description = "Invalid fields, or `invalid_handle` with a `hint`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
pub(super) async fn update_team(
    State(state): State<AppState>,
    path: TeamPath,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<TeamPatchDto>,
) -> Result<Json<TeamDto>, ApiError> {
    let detail = state
        .services
        .update_team(path.principal, path.team_id, to_team_patch(&body)?, expected)
        .await?;
    Ok(Json(to_team_dto(detail)))
}

/// Adds an existing user to the team as a member. Admins only. Answers `200` with the team unchanged when they are
/// already a member.
#[utoipa::path(post, path = "/teams/{team}/members", tag = "teams", security(("session" = [])),
    params(TeamParams), request_body = AddMemberDto,
    responses(
        (status = 201, body = TeamDto),
        (status = 200, description = "Already a member", body = TeamDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`personal_team`", body = Problem),
        (status = 422, description = "`user_not_found`", body = Problem),
    ))]
async fn add_member(
    State(state): State<AppState>,
    path: TeamPath,
    ApiJson(body): ApiJson<AddMemberDto>,
) -> Result<(StatusCode, Json<TeamDto>), ApiError> {
    let (detail, outcome) = state
        .services
        .add_member(path.principal, path.team_id, UserId::from_uuid(body.user_id))
        .await?;
    let status = match outcome {
        MemberAdded::Added => StatusCode::CREATED,
        MemberAdded::AlreadyMember => StatusCode::OK,
    };
    Ok((status, Json(to_team_dto(detail))))
}

/// Makes a member an admin, or an admin a member. Admins only.
#[utoipa::path(patch, path = "/teams/{team}/members/{username}", tag = "teams", security(("session" = [])),
    params(MemberParams), request_body = MemberPatchDto,
    responses(
        (status = 200, body = TeamDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`last_admin`", body = Problem),
    ))]
async fn update_member(
    State(state): State<AppState>,
    path: MemberPath,
    ApiJson(body): ApiJson<MemberPatchDto>,
) -> Result<Json<TeamDto>, ApiError> {
    let detail = state
        .services
        .change_member_role(path.principal, path.team_id, path.user_id, body.role.into())
        .await?;
    Ok(Json(to_team_dto(detail)))
}

/// Takes a user out of the team. Admins only. Removing someone who isn't a member succeeds.
#[utoipa::path(delete, path = "/teams/{team}/members/{username}", tag = "teams", security(("session" = [])),
    params(MemberParams),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`last_admin` or `personal_team`", body = Problem),
    ))]
async fn remove_member(State(state): State<AppState>, path: MemberPath) -> Result<StatusCode, ApiError> {
    state
        .services
        .remove_member(path.principal, path.team_id, path.user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Leaves the team. The last admin can't leave until another member is an admin.
#[utoipa::path(post, path = "/teams/{team}/leave", tag = "teams", security(("session" = [])),
    params(TeamParams),
    responses(
        (status = 204, description = "Left the team"),
        (status = 404, body = Problem),
        (status = 409, description = "`last_admin` or `personal_team`", body = Problem),
    ))]
async fn leave_team(State(state): State<AppState>, path: TeamPath) -> Result<StatusCode, ApiError> {
    state.services.leave_team(path.principal, path.team_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
