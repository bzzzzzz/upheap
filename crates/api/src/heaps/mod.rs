//! Heaps: reading, renaming, archiving and unarchiving them. New heaps are created under their team
//! (`POST /teams/{team}/heaps`).

use axum::Json;
use axum::extract::State;
use domain::handle::Handle;
use domain::permission::HeapAction;
use domain::text::Name;
use domain::validation::Field;
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::heaps::HeapPatch;
use service::teams::HeapView;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

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

/// An action the caller can take on a heap.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = HeapAction)]
pub(crate) enum HeapActionDto {
    Rename,
    Archive,
    Unarchive,
    ViewHistory,
}

impl From<HeapAction> for HeapActionDto {
    fn from(action: HeapAction) -> Self {
        match action {
            HeapAction::Rename => Self::Rename,
            HeapAction::Archive => Self::Archive,
            HeapAction::Unarchive => Self::Unarchive,
            HeapAction::ViewHistory => Self::ViewHistory,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = Heap)]
pub(crate) struct HeapDto {
    pub id: Uuid,
    pub team_id: Uuid,
    /// Unique within the team; the heap's path is `/teams/{team}/heaps/{slug}`.
    pub slug: String,
    pub name: String,
    /// When it was archived, in UTC milliseconds; `null` while it is active. An archived heap is read-only.
    pub archived_at: Option<i64>,
    /// Items the heap's list shows by default, before filters: open and in progress ones, as the list's
    /// `unfiltered_count` with the default view.
    pub open_count: u32,
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
            slug: heap.slug.to_string(),
            name: heap.name.as_str().to_owned(),
            archived_at: heap.archived_at.map(domain::Timestamp::as_millis),
            open_count: view.open_count,
            version: heap.version.get(),
            allowed_actions: view.allowed_actions.into_iter().map(HeapActionDto::from).collect(),
        }
    }
}

/// A change to a heap. Leave a field out to keep it.
#[derive(Deserialize, ToSchema)]
#[schema(as = HeapPatchRequest)]
pub(crate) struct HeapPatchDto {
    /// 1 to 100 characters.
    pub name: Option<String>,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Links with the old
    /// slug stop working.
    pub slug: Option<String>,
}

impl HeapPatchDto {
    fn to_patch(&self) -> Result<HeapPatch, ServiceError> {
        Ok(HeapPatch {
            name: self.name.as_deref().map(Name::parse).transpose()?,
            slug: self
                .slug
                .as_deref()
                .map(|raw| Handle::parse(raw, Field::Slug))
                .transpose()?,
        })
    }
}

#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}", tag = "heaps", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = HeapDto), (status = 404, body = Problem)))]
async fn get_heap(State(state): State<AppState>, path: HeapPath) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.load_heap(path.principal, path.heap.heap_id).await?;
    Ok(Json(view.into()))
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
        .update_heap(path.principal, path.heap.heap_id, body.to_patch()?, expected)
        .await?;
    Ok(Json(view.into()))
}

/// Archives the heap, making it read-only for everyone. Admins only. Archiving an archived heap succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/archive", tag = "heaps", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = HeapDto), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn archive_heap(State(state): State<AppState>, path: HeapPath) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.archive_heap(path.principal, path.heap.heap_id).await?;
    Ok(Json(view.into()))
}

/// Makes an archived heap writable again. Admins only. Unarchiving an active heap succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/unarchive", tag = "heaps", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = HeapDto), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn unarchive_heap(State(state): State<AppState>, path: HeapPath) -> Result<Json<HeapDto>, ApiError> {
    let view = state.services.unarchive_heap(path.principal, path.heap.heap_id).await?;
    Ok(Json(view.into()))
}
