//! Heaps: reading, renaming, archiving and unarchiving them. New heaps are created under their team
//! (`POST /teams/{team}/heaps`).

use api_types::heaps::{HeapActionDto, HeapDto, HeapPatchDto};
use axum::Json;
use axum::extract::State;
use domain::handle::Handle;
use domain::text::Name;
use domain::validation::Field;
use service::ServiceError;
use service::heaps::HeapPatch;
use service::teams::HeapView;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, IfMatch};
use crate::paths::{HeapParams, HeapPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_heap, update_heap))
        .routes(routes!(archive_heap))
        .routes(routes!(unarchive_heap))
}

pub(crate) fn to_heap_dto(view: HeapView) -> HeapDto {
    let heap = view.heap;
    HeapDto {
        id: *heap.id.as_uuid(),
        team_id: *heap.team_id.as_uuid(),
        slug: heap.slug.to_string(),
        name: heap.name.as_str().to_owned(),
        archived_at: heap.archived_at.map(domain::Timestamp::as_millis),
        open_count: view.open_count,
        version: heap.version.get(),
        allowed_actions: view.allowed_actions.into_iter().map(HeapActionDto::from).collect(),
    }
}

fn to_heap_patch(body: &HeapPatchDto) -> Result<HeapPatch, ServiceError> {
    Ok(HeapPatch {
        name: body.name.as_deref().map(Name::parse).transpose()?,
        slug: body
            .slug
            .as_deref()
            .map(|raw| Handle::parse(raw, Field::Slug))
            .transpose()?,
    })
}

#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}", tag = "heaps", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = HeapDto), (status = 404, body = Problem)))]
async fn get_heap(State(state): State<AppState>, path: HeapPath) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.load_heap(path.principal, path.heap.heap_id).await?;
    Ok(Json(to_heap_dto(view)))
}

/// Renames the heap or changes its slug. Admins only; send the heap's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}", tag = "heaps", security(("session" = [])),
    params(HeapParams, IfMatchHeader), request_body = HeapPatchDto,
    responses(
        (status = 200, body = HeapDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `heap_archived` or `handle_taken` (`field` `slug`)", body = Problem),
        (status = 422, description = "Invalid fields, or `invalid_handle` with a `hint`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_heap(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<HeapPatchDto>,
) -> Result<Json<HeapDto>, ApiError> {
    let view = state
        .services
        .update_heap(path.principal, path.heap.heap_id, to_heap_patch(&body)?, expected)
        .await?;
    Ok(Json(to_heap_dto(view)))
}

/// Archives the heap, making it read-only for everyone. Admins only. Archiving an archived heap succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/archive", tag = "heaps", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = HeapDto), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn archive_heap(State(state): State<AppState>, path: HeapPath) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.archive_heap(path.principal, path.heap.heap_id).await?;
    Ok(Json(to_heap_dto(view)))
}

/// Makes an archived heap writable again. Admins only. Unarchiving an active heap succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/unarchive", tag = "heaps", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = HeapDto), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn unarchive_heap(State(state): State<AppState>, path: HeapPath) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.unarchive_heap(path.principal, path.heap.heap_id).await?;
    Ok(Json(to_heap_dto(view)))
}
