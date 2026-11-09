//! Teams: reading them and adding heaps to them. Changing teams and their members is in `manage`, which needs
//! accounts.

#[cfg(feature = "accounts")]
mod manage;

use api_types::heaps::HeapDto;
use api_types::teams::{CreateNamedDto, MemberActionDto, MemberDto, TeamActionDto, TeamDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::handle::Handle;
use domain::permission::TeamAction;
use domain::text::Name;
use domain::validation::Field;
use service::ServiceError;
use service::teams::{MemberView, TeamDetail};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::ApiJson;
use crate::heaps::to_heap_dto;
use crate::paths::{TeamParams, TeamPath};
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

fn to_member_dto(view: MemberView) -> MemberDto {
    MemberDto {
        user_id: *view.user.id.as_uuid(),
        username: view.user.username.to_string(),
        display_name: view.user.display_name.as_str().to_owned(),
        email: view.user.email.as_str().to_owned(),
        role: view.membership.role.into(),
        joined_at: view.membership.joined_at.as_millis(),
        allowed_actions: view.allowed_actions.into_iter().map(MemberActionDto::from).collect(),
    }
}

/// The team as `authentication`'s caller sees it. The local user has no one to add and only the one team, so
/// creating heaps is the only team action offered, and no member actions are.
pub(crate) fn to_team_dto(mut detail: TeamDetail, authentication: &Authentication) -> TeamDto {
    if authentication.is_single_user() {
        detail
            .allowed_actions
            .retain(|action| matches!(action, TeamAction::CreateHeap));
        for member in &mut detail.members {
            member.allowed_actions.clear();
        }
    }
    TeamDto {
        id: *detail.team.id.as_uuid(),
        slug: detail.team.slug.to_string(),
        name: detail.team.name.as_str().to_owned(),
        personal: detail.team.is_personal(),
        version: detail.team.version.get(),
        role: detail.role.into(),
        members: detail.members.into_iter().map(to_member_dto).collect(),
        heaps: detail.heaps.into_iter().map(to_heap_dto).collect(),
        allowed_actions: detail.allowed_actions.into_iter().map(TeamActionDto::from).collect(),
    }
}

/// The name of a new team or heap.
pub(crate) fn to_name(body: &CreateNamedDto) -> Result<Name, ServiceError> {
    Ok(Name::parse(&body.name)?)
}

/// The slug of a new team or heap, when one was given.
pub(crate) fn to_slug(body: &CreateNamedDto) -> Result<Option<Handle>, ServiceError> {
    Ok(body
        .slug
        .as_deref()
        .map(|raw| Handle::parse(raw, Field::Slug))
        .transpose()?)
}

/// The team with its members and heaps.
#[utoipa::path(get, path = "/teams/{team}", tag = "teams", security(("session" = [])),
    params(TeamParams),
    responses((status = 200, body = TeamDto), (status = 404, body = Problem)))]
async fn get_team(State(state): State<AppState>, path: TeamPath) -> Result<Json<TeamDto>, ApiError> {
    let detail = state.services.load_team(path.principal, path.team_id).await?;
    Ok(Json(to_team_dto(detail, &state.authentication)))
}

/// Adds a heap to the team. Admins only.
#[utoipa::path(post, path = "/teams/{team}/heaps", tag = "heaps", security(("session" = [])),
    params(TeamParams), request_body = CreateNamedDto,
    responses(
        (status = 201, body = HeapDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`handle_taken` (`field` `slug`)", body = Problem),
        (status = 422, description = "Invalid fields, or `invalid_handle` with a `hint`", body = Problem),
    ))]
async fn create_heap(
    State(state): State<AppState>,
    path: TeamPath,
    ApiJson(body): ApiJson<CreateNamedDto>,
) -> Result<(StatusCode, Json<HeapDto>), ApiError> {
    let view = state
        .services
        .create_heap(path.principal, path.team_id, to_name(&body)?, to_slug(&body)?)
        .await?;
    Ok((StatusCode::CREATED, Json(to_heap_dto(view))))
}
