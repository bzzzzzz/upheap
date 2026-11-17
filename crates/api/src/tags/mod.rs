//! A heap's tags and a team's: listing, creating, renaming and describing them. Tags are never deleted.

use api_types::common::HeapSummaryDto;
use api_types::tags::{
    CreateTagDto, TagActionDto, TagDto, TagKindDto, TagListActionDto, TagListDto, TagPatchDto, TeamTagListDto,
};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::TagId;
use domain::tag::TagOwner;
use serde::Deserialize;
use service::ServiceError;
use service::tags::TagView;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, IfMatch};
use crate::paths::{HeapParams, HeapPath, TeamParams, TeamPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_tags, create_tag))
        .routes(routes!(update_tag))
        .routes(routes!(list_team_tags, create_team_tag))
        .routes(routes!(update_team_tag))
}

/// `{tag_id}` in a path below the heap's.
#[derive(Deserialize)]
struct TagPathId {
    tag_id: Uuid,
}

fn to_tag_dto(view: TagView) -> TagDto {
    let tag = view.tag;
    let (kind, heap_id) = match tag.owner {
        TagOwner::Heap(heap_id) => (TagKindDto::Heap, Some(*heap_id.as_uuid())),
        TagOwner::Team(_) => (TagKindDto::Team, None),
    };
    TagDto {
        id: *tag.id.as_uuid(),
        kind,
        heap_id,
        name: tag.name.as_str().to_owned(),
        description: tag.description.map(|d| d.as_str().to_owned()),
        version: tag.version.get(),
        allowed_actions: view.allowed_actions.into_iter().map(TagActionDto::from).collect(),
    }
}

/// The heap's own tags and its team's, each by name.
#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/tags", tag = "tags", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = TagListDto), (status = 404, body = Problem)))]
async fn list_tags(State(state): State<AppState>, path: HeapPath) -> Result<Json<TagListDto>, ApiError> {
    let list = state.services.list_tags(path.principal, path.heap.heap_id).await?;
    Ok(Json(TagListDto {
        heap: HeapSummaryDto::from(&list.heap),
        tags: list.tags.into_iter().map(to_tag_dto).collect(),
        team_tags: list.team_tags.into_iter().map(to_tag_dto).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(TagListActionDto::from).collect(),
    }))
}

/// Adds a tag to the heap.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/tags", tag = "tags", security(("session" = [])),
    params(HeapParams), request_body = CreateTagDto,
    responses(
        (status = 201, body = TagDto),
        (status = 404, body = Problem),
        (status = 409, description = "`tag_name_taken`, `tag_name_taken_by_team_tag` or `heap_archived`", body = Problem),
        (status = 422, body = Problem),
    ))]
async fn create_tag(
    State(state): State<AppState>,
    path: HeapPath,
    ApiJson(body): ApiJson<CreateTagDto>,
) -> Result<(StatusCode, Json<TagDto>), ApiError> {
    let fields = body.to_new_tag().map_err(ServiceError::from)?;
    let view = state
        .services
        .create_tag(path.principal, path.heap.heap_id, fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_tag_dto(view))))
}

/// Renames a heap's tag or changes its description. A team's tag is edited under the team's tags. Send the tag's
/// `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/tags/{tag_id}", tag = "tags", security(("session" = [])),
    params(HeapParams, ("tag_id" = Uuid, Path), IfMatchHeader), request_body = TagPatchDto,
    responses(
        (status = 200, body = TagDto),
        (status = 404, body = Problem),
        (status = 409, description = "`tag_name_taken`, `tag_name_taken_by_team_tag`, `version_conflict` or `heap_archived`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_tag(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiPath(TagPathId { tag_id }): ApiPath<TagPathId>,
    ApiJson(body): ApiJson<TagPatchDto>,
) -> Result<Json<TagDto>, ApiError> {
    let patch = body.to_patch().map_err(ServiceError::from)?;
    let view = state
        .services
        .update_tag(
            path.principal,
            path.heap.heap_id,
            TagId::from_uuid(tag_id),
            patch,
            expected,
        )
        .await?;
    Ok(Json(to_tag_dto(view)))
}

/// The team's tags, by name. Every heap of the team has them.
#[utoipa::path(get, path = "/teams/{team}/tags", tag = "tags", security(("session" = [])),
    params(TeamParams),
    responses((status = 200, body = TeamTagListDto), (status = 404, body = Problem)))]
async fn list_team_tags(State(state): State<AppState>, path: TeamPath) -> Result<Json<TeamTagListDto>, ApiError> {
    let list = state.services.list_team_tags(path.principal, path.team_id).await?;
    Ok(Json(TeamTagListDto {
        tags: list.tags.into_iter().map(to_tag_dto).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(TagListActionDto::from).collect(),
    }))
}

/// Adds a tag to the team, for all its heaps. The name must be free among the team's tags and the tags of every
/// heap of the team.
#[utoipa::path(post, path = "/teams/{team}/tags", tag = "tags", security(("session" = [])),
    params(TeamParams), request_body = CreateTagDto,
    responses(
        (status = 201, body = TagDto),
        (status = 404, body = Problem),
        (status = 409, description = "`tag_name_taken`", body = Problem),
        (status = 422, body = Problem),
    ))]
async fn create_team_tag(
    State(state): State<AppState>,
    path: TeamPath,
    ApiJson(body): ApiJson<CreateTagDto>,
) -> Result<(StatusCode, Json<TagDto>), ApiError> {
    let fields = body.to_new_tag().map_err(ServiceError::from)?;
    let view = state
        .services
        .create_team_tag(path.principal, path.team_id, fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_tag_dto(view))))
}

/// Renames a team's tag or changes its description, in every heap at once. Send the tag's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/tags/{tag_id}", tag = "tags", security(("session" = [])),
    params(TeamParams, ("tag_id" = Uuid, Path), IfMatchHeader), request_body = TagPatchDto,
    responses(
        (status = 200, body = TagDto),
        (status = 404, body = Problem),
        (status = 409, description = "`tag_name_taken` or `version_conflict`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_team_tag(
    State(state): State<AppState>,
    path: TeamPath,
    IfMatch(expected): IfMatch,
    ApiPath(TagPathId { tag_id }): ApiPath<TagPathId>,
    ApiJson(body): ApiJson<TagPatchDto>,
) -> Result<Json<TagDto>, ApiError> {
    let patch = body.to_patch().map_err(ServiceError::from)?;
    let view = state
        .services
        .update_team_tag(path.principal, path.team_id, TagId::from_uuid(tag_id), patch, expected)
        .await?;
    Ok(Json(to_tag_dto(view)))
}
