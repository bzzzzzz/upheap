//! A heap's saved views: listing them, saving one, changing it and archiving it. Views are never deleted.

use api_types::common::HeapSummaryDto;
use api_types::list_view::ListViewDto;
use api_types::views::{CreateSavedViewDto, SavedViewDto, SavedViewListDto, SavedViewPatchDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::parameter::ParameterError;
use domain::saved_view::NewSavedView;
use domain::text::Name;
use domain::{SavedViewId, ValidationError};
use serde::Deserialize;
use service::ServiceError;
use service::saved_views::SavedViewEntry;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, IfMatch};
use crate::paths::{HeapParams, HeapPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_views, create_view))
        .routes(routes!(update_view))
        .routes(routes!(archive_view))
}

/// `{view_id}` in a path below the heap's.
#[derive(Deserialize)]
struct ViewPathId {
    view_id: Uuid,
}

fn to_saved_view_dto(entry: SavedViewEntry) -> SavedViewDto {
    let view = entry.view;
    SavedViewDto {
        id: *view.id.as_uuid(),
        heap_id: *view.heap_id.as_uuid(),
        name: view.name.as_str().to_owned(),
        view: ListViewDto::from(&view.list_view),
        created_by: *view.created_by.as_uuid(),
        archived_at: view.archived_at.map(|t| t.as_millis()),
        version: view.version.get(),
        allowed_actions: entry.allowed_actions.into_iter().map(Into::into).collect(),
    }
}

/// The heap's saved views that are in use, by name.
#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/views", tag = "views", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = SavedViewListDto), (status = 404, body = Problem)))]
async fn list_views(State(state): State<AppState>, path: HeapPath) -> Result<Json<SavedViewListDto>, ApiError> {
    let list = state
        .services
        .list_saved_views(path.principal, path.heap.heap_id)
        .await?;
    Ok(Json(SavedViewListDto {
        heap: HeapSummaryDto::from(&list.heap),
        views: list.views.into_iter().map(to_saved_view_dto).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(Into::into).collect(),
    }))
}

/// Saves a view of the heap, shared with everyone in the team.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/views", tag = "views", security(("session" = [])),
    params(HeapParams), request_body = CreateSavedViewDto,
    responses(
        (status = 201, body = SavedViewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
        (status = 422, description = "`tag_not_in_heap`, `too_many_tags` (`field` `view.tag`), `option_not_in_heap`, `too_many_options` (`view.param`), `parameter_not_in_heap`, `sort_parameter_mismatch` (`view.sort_parameter`), or an invalid name", body = Problem),
    ))]
async fn create_view(
    State(state): State<AppState>,
    path: HeapPath,
    ApiJson(body): ApiJson<CreateSavedViewDto>,
) -> Result<(StatusCode, Json<SavedViewDto>), ApiError> {
    let fields = NewSavedView {
        name: Name::parse(&body.name).map_err(ServiceError::from)?,
        list_view: body.view.to_view().map_err(|e| name_view_field(e.into()))?,
    };
    let entry = state
        .services
        .create_saved_view(path.principal, path.heap.heap_id, fields)
        .await
        .map_err(name_view_field)?;
    Ok((StatusCode::CREATED, Json(to_saved_view_dto(entry))))
}

/// Renames a saved view or replaces what it shows. Send the view's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/views/{view_id}", tag = "views", security(("session" = [])),
    params(HeapParams, ("view_id" = Uuid, Path), IfMatchHeader), request_body = SavedViewPatchDto,
    responses(
        (status = 200, body = SavedViewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `saved_view_archived` or `heap_archived`", body = Problem),
        (status = 422, description = "`tag_not_in_heap`, `too_many_tags` (`field` `view.tag`), `option_not_in_heap`, `too_many_options` (`view.param`), `parameter_not_in_heap`, `sort_parameter_mismatch` (`view.sort_parameter`), or an invalid name", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_view(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiPath(ViewPathId { view_id }): ApiPath<ViewPathId>,
    ApiJson(body): ApiJson<SavedViewPatchDto>,
) -> Result<Json<SavedViewDto>, ApiError> {
    let patch = body.to_patch().map_err(|e| name_view_field(e.into()))?;
    let entry = state
        .services
        .update_saved_view(
            path.principal,
            path.heap.heap_id,
            SavedViewId::from_uuid(view_id),
            patch,
            expected,
        )
        .await
        .map_err(name_view_field)?;
    Ok(Json(to_saved_view_dto(entry)))
}

/// Archives a saved view, which takes it out of the heap's list. Archiving an archived view succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/views/{view_id}/archive", tag = "views", security(("session" = [])),
    params(HeapParams, ("view_id" = Uuid, Path)),
    responses(
        (status = 200, body = SavedViewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
    ))]
async fn archive_view(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(ViewPathId { view_id }): ApiPath<ViewPathId>,
) -> Result<Json<SavedViewDto>, ApiError> {
    let entry = state
        .services
        .archive_saved_view(path.principal, path.heap.heap_id, SavedViewId::from_uuid(view_id))
        .await?;
    Ok(Json(to_saved_view_dto(entry)))
}

/// Names the field of `view` an error comes from: a foreign or excess tag from its tag filter, a foreign or excess
/// option from its option filter, and a foreign or misplaced sort parameter from `sort_parameter`.
fn name_view_field(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::TagNotInHeap { .. } | ServiceError::Invalid(ValidationError::TooManyTags { .. }) => "view.tag",
        ServiceError::OptionNotInHeap { .. } | ServiceError::Invalid(ValidationError::TooManyOptions { .. }) => {
            "view.param"
        }
        ServiceError::Parameter(ParameterError::NotInHeap { .. })
        | ServiceError::Invalid(ValidationError::SortParameterMismatch) => "view.sort_parameter",
        _ => return error.into(),
    };
    ApiError::InField { error, field }
}
