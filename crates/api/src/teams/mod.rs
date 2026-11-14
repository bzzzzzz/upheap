//! Teams: reading them and adding heaps to them. Changing teams and their members is in `manage`, and their invite
//! links in `invite_links`.

mod invite_links;
mod manage;

use api_types::heaps::HeapDto;
use api_types::teams::{CreateHeapDto, CreateNamedDto, MemberActionDto, MemberDto, TeamActionDto, TeamDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::ItemId;
use domain::handle::Handle;
use domain::text::{Context, Name};
use domain::validation::Field;
use service::ServiceError;
use service::teams::{MemberView, TeamDetail};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::ApiJson;
use crate::heaps::to_heap_dto;
use crate::paths::{TeamParams, TeamPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    // `GET` and `PATCH` share a path, so they are registered together.
    OpenApiRouter::new()
        .routes(routes!(get_team, manage::update_team))
        .merge(manage::routes())
        .merge(invite_links::routes())
        .routes(routes!(create_heap))
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

pub(crate) fn to_team_dto(detail: TeamDetail) -> TeamDto {
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

/// The name, slug and context of a new heap.
fn to_new_heap(body: &CreateHeapDto) -> Result<(Name, Option<Handle>, Option<Context>), ServiceError> {
    let slug = body
        .slug
        .as_deref()
        .map(|raw| Handle::parse(raw, Field::Slug))
        .transpose()?;
    let context = body.context.as_deref().map(Context::parse).transpose()?.flatten();
    Ok((Name::parse(&body.name)?, slug, context))
}

/// The team with its members and heaps.
#[utoipa::path(get, path = "/teams/{team}", tag = "teams", security(("session" = [])),
    params(TeamParams),
    responses((status = 200, body = TeamDto), (status = 404, body = Problem)))]
async fn get_team(State(state): State<AppState>, path: TeamPath) -> Result<Json<TeamDto>, ApiError> {
    let detail = state.services.load_team(path.principal, path.team_id).await?;
    Ok(Json(to_team_dto(detail)))
}

/// Adds a heap to the team. Admins only. With `origin_id` the heap is made from that open note of the team, which stays
/// open until `became`; a heap made from a note is in the queue of whoever works the note's heap. Inside a run the
/// heap must be made from the run's item, with a `context`, and the run goes on.
#[utoipa::path(post, path = "/teams/{team}/heaps", tag = "heaps", security(("session" = [])),
    params(TeamParams), request_body = CreateHeapDto,
    responses(
        (status = 201, body = HeapDto),
        (status = 403, description = "A member, or from inside a run: `run_forbidden` (no `origin_id`), `run_add_needs_context` (no `context`) or `run_wrong_item` (`origin_id` is not the run's item, named in `item_key`)", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`handle_taken` (`field` `slug`), `origin_not_a_note` or `origin_closed` (`origin_id`), or from inside a run `run_over`", body = Problem),
        (status = 422, description = "Invalid fields, or `invalid_handle` with a `hint`", body = Problem),
    ))]
async fn create_heap(
    State(state): State<AppState>,
    path: TeamPath,
    ApiJson(body): ApiJson<CreateHeapDto>,
) -> Result<(StatusCode, Json<HeapDto>), ApiError> {
    let (name, slug, context) = to_new_heap(&body)?;
    let view = state
        .services
        .create_heap(
            path.principal,
            path.team_id,
            name,
            slug,
            context,
            body.origin_id.map(ItemId::from_uuid),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(to_heap_dto(view))))
}
