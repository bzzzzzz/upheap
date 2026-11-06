//! A heap's saved views: listing them, saving one, changing it and archiving it. Views are never deleted.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::permission::{SavedViewAction, SavedViewListAction};
use domain::saved_view::{NewSavedView, SavedViewPatch};
use domain::text::Name;
use domain::{HeapId, SavedViewId, ValidationError};
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::saved_views::SavedViewEntry;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{HeapSummaryDto, IfMatchHeader};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, CurrentUser, IfMatch};
use crate::list_view::{ListViewDto, ListViewInputDto};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_views, create_view))
        .routes(routes!(update_view))
        .routes(routes!(archive_view))
}

/// An action the caller can take on a saved view.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SavedViewAction)]
pub(crate) enum SavedViewActionDto {
    Edit,
    Archive,
}

impl From<SavedViewAction> for SavedViewActionDto {
    fn from(action: SavedViewAction) -> Self {
        match action {
            SavedViewAction::Edit => Self::Edit,
            SavedViewAction::Archive => Self::Archive,
        }
    }
}

/// An action the caller can take on a heap's saved views.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SavedViewListAction)]
pub(crate) enum SavedViewListActionDto {
    Create,
}

impl From<SavedViewListAction> for SavedViewListActionDto {
    fn from(action: SavedViewListAction) -> Self {
        match action {
            SavedViewListAction::Create => Self::Create,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = SavedView)]
pub(crate) struct SavedViewDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub name: String,
    /// What the view shows. The fields are the query parameters of `GET /heaps/{heap_id}/items`.
    pub view: ListViewDto,
    pub created_by: Uuid,
    /// When the view was archived (UTC milliseconds); `null` while it is in use.
    pub archived_at: Option<i64>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    pub allowed_actions: Vec<SavedViewActionDto>,
}

impl From<SavedViewEntry> for SavedViewDto {
    fn from(entry: SavedViewEntry) -> Self {
        let view = entry.view;
        Self {
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
}

#[derive(Serialize, ToSchema)]
#[schema(as = SavedViewList)]
pub(crate) struct SavedViewListDto {
    pub heap: HeapSummaryDto,
    /// The views in use, by name. Archived views are left out.
    pub views: Vec<SavedViewDto>,
    pub allowed_actions: Vec<SavedViewListActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CreateSavedViewRequest)]
pub(crate) struct CreateSavedViewDto {
    /// 1 to 100 characters. Need not be unique.
    pub name: String,
    #[serde(default)]
    pub view: ListViewInputDto,
}

/// A partial edit. Leave a field out to keep it; a `view` replaces the whole view, with defaults for what it leaves
/// out.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SavedViewPatchRequest)]
pub(crate) struct SavedViewPatchDto {
    pub name: Option<String>,
    pub view: Option<ListViewInputDto>,
}

/// The heap's saved views that are in use, by name.
#[utoipa::path(get, path = "/heaps/{heap_id}/views", tag = "views", security(("session" = [])),
    params(("heap_id" = Uuid, Path)),
    responses((status = 200, body = SavedViewListDto), (status = 404, body = Problem)))]
async fn list_views(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
) -> Result<Json<SavedViewListDto>, ApiError> {
    let list = state
        .services
        .list_saved_views(principal, HeapId::from_uuid(heap_id))
        .await?;
    Ok(Json(SavedViewListDto {
        heap: HeapSummaryDto::from(&list.heap),
        views: list.views.into_iter().map(SavedViewDto::from).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(Into::into).collect(),
    }))
}

/// Saves a view of the heap, shared with everyone in the team.
#[utoipa::path(post, path = "/heaps/{heap_id}/views", tag = "views", security(("session" = [])),
    params(("heap_id" = Uuid, Path)), request_body = CreateSavedViewDto,
    responses(
        (status = 201, body = SavedViewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
        (status = 422, description = "`tag_not_in_heap` with `field` `view.tag`, or an invalid name", body = Problem),
    ))]
async fn create_view(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateSavedViewDto>,
) -> Result<(StatusCode, Json<SavedViewDto>), ApiError> {
    let fields = NewSavedView {
        name: Name::parse(&body.name).map_err(ServiceError::from)?,
        list_view: body.view.to_view(),
    };
    let entry = state
        .services
        .create_saved_view(principal, HeapId::from_uuid(heap_id), fields)
        .await
        .map_err(name_view_tag_field)?;
    Ok((StatusCode::CREATED, Json(SavedViewDto::from(entry))))
}

/// Renames a saved view or replaces what it shows. Send the view's `version` in `If-Match`.
#[utoipa::path(patch, path = "/views/{view_id}", tag = "views", security(("session" = [])),
    params(("view_id" = Uuid, Path), IfMatchHeader), request_body = SavedViewPatchDto,
    responses(
        (status = 200, body = SavedViewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `saved_view_archived` or `heap_archived`", body = Problem),
        (status = 422, description = "`tag_not_in_heap` with `field` `view.tag`, or an invalid name", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_view(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiPath(view_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<SavedViewPatchDto>,
) -> Result<Json<SavedViewDto>, ApiError> {
    let patch = to_patch(&body).map_err(ServiceError::from)?;
    let entry = state
        .services
        .update_saved_view(principal, SavedViewId::from_uuid(view_id), patch, expected)
        .await
        .map_err(name_view_tag_field)?;
    Ok(Json(SavedViewDto::from(entry)))
}

/// Archives a saved view, which takes it out of the heap's list. Archiving an archived view succeeds.
#[utoipa::path(post, path = "/views/{view_id}/archive", tag = "views", security(("session" = [])),
    params(("view_id" = Uuid, Path)),
    responses(
        (status = 200, body = SavedViewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
    ))]
async fn archive_view(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(view_id): ApiPath<Uuid>,
) -> Result<Json<SavedViewDto>, ApiError> {
    let entry = state
        .services
        .archive_saved_view(principal, SavedViewId::from_uuid(view_id))
        .await?;
    Ok(Json(SavedViewDto::from(entry)))
}

fn to_patch(body: &SavedViewPatchDto) -> Result<SavedViewPatch, ValidationError> {
    Ok(SavedViewPatch {
        name: body.name.as_deref().map(Name::parse).transpose()?,
        list_view: body.view.as_ref().map(ListViewInputDto::to_view),
    })
}

/// A foreign tag, or too many tags, can only come from the view's tag filter.
fn name_view_tag_field(error: ServiceError) -> ApiError {
    match error {
        ServiceError::TagNotInHeap { .. } | ServiceError::Invalid(ValidationError::TooManyTags { .. }) => {
            ApiError::InField {
                error,
                field: "view.tag",
            }
        }
        other => other.into(),
    }
}
