//! Heaps: reading, renaming, archiving and unarchiving them. New heaps are created under their team
//! (`POST /teams/{team_id}/heaps`).

use axum::Json;
use axum::extract::State;
use domain::HeapId;
use domain::permission::HeapAction;
use serde::Serialize;
use service::teams::HeapView;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, CurrentUser, IfMatch};
use crate::teams::NameDto;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_heap, update_heap))
        .routes(routes!(archive_heap))
        .routes(routes!(unarchive_heap))
}

/// An action the caller can take on a heap.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = HeapAction)]
pub(crate) enum HeapActionDto {
    Rename,
    Archive,
    Unarchive,
}

impl From<HeapAction> for HeapActionDto {
    fn from(action: HeapAction) -> Self {
        match action {
            HeapAction::Rename => Self::Rename,
            HeapAction::Archive => Self::Archive,
            HeapAction::Unarchive => Self::Unarchive,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = Heap)]
pub(crate) struct HeapDto {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    /// When it was archived, in UTC milliseconds; `null` while it is active. An archived heap is read-only.
    pub archived_at: Option<i64>,
    /// Send back in `If-Match` when renaming.
    pub version: u32,
    pub allowed_actions: Vec<HeapActionDto>,
}

impl From<HeapView> for HeapDto {
    fn from(view: HeapView) -> Self {
        let heap = view.heap;
        Self {
            id: *heap.id.as_uuid(),
            team_id: *heap.team_id.as_uuid(),
            name: heap.name.as_str().to_owned(),
            archived_at: heap.archived_at.map(domain::Timestamp::as_millis),
            version: heap.version.get(),
            allowed_actions: view.allowed_actions.into_iter().map(HeapActionDto::from).collect(),
        }
    }
}

#[utoipa::path(get, path = "/heaps/{heap_id}", tag = "heaps", security(("session" = [])),
    params(("heap_id" = Uuid, Path)),
    responses((status = 200, body = HeapDto), (status = 404, body = Problem)))]
async fn get_heap(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.load_heap(principal, HeapId::from_uuid(heap_id)).await?;
    Ok(Json(view.into()))
}

/// Renames the heap. Admins only; send the heap's `version` in `If-Match`.
#[utoipa::path(patch, path = "/heaps/{heap_id}", tag = "heaps", security(("session" = [])),
    params(("heap_id" = Uuid, Path), IfMatchHeader), request_body = NameDto,
    responses(
        (status = 200, body = HeapDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict` or `heap_archived`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_heap(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiPath(heap_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<NameDto>,
) -> Result<Json<HeapDto>, ApiError> {
    let view = state
        .services
        .rename_heap(principal, HeapId::from_uuid(heap_id), body.to_name()?, expected)
        .await?;
    Ok(Json(view.into()))
}

/// Archives the heap, making it read-only for everyone. Admins only. Archiving an archived heap succeeds.
#[utoipa::path(post, path = "/heaps/{heap_id}/archive", tag = "heaps", security(("session" = [])),
    params(("heap_id" = Uuid, Path)),
    responses((status = 200, body = HeapDto), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn archive_heap(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
) -> Result<Json<HeapDto>, ApiError> {
    let view = state
        .services
        .archive_heap(principal, HeapId::from_uuid(heap_id))
        .await?;
    Ok(Json(view.into()))
}

/// Makes an archived heap writable again. Admins only. Unarchiving an active heap succeeds.
#[utoipa::path(post, path = "/heaps/{heap_id}/unarchive", tag = "heaps", security(("session" = [])),
    params(("heap_id" = Uuid, Path)),
    responses((status = 200, body = HeapDto), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn unarchive_heap(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
) -> Result<Json<HeapDto>, ApiError> {
    let view = state
        .services
        .unarchive_heap(principal, HeapId::from_uuid(heap_id))
        .await?;
    Ok(Json(view.into()))
}
