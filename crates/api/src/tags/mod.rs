//! A heap's tags: listing, creating, renaming and describing them. Tags are never deleted.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::permission::{TagAction, TagListAction};
use domain::tag::{NewTag, TagDescription, TagName, TagPatch};
use domain::{HeapId, TagId, ValidationError};
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::tags::TagView;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{HeapSummaryDto, IfMatchHeader, double_option};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, CurrentUser, IfMatch};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_tags, create_tag))
        .routes(routes!(update_tag))
}

/// An action the caller can take on a tag.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagAction)]
pub(crate) enum TagActionDto {
    Edit,
}

impl From<TagAction> for TagActionDto {
    fn from(action: TagAction) -> Self {
        match action {
            TagAction::Edit => Self::Edit,
        }
    }
}

/// An action the caller can take on a heap's tag list.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagListAction)]
pub(crate) enum TagListActionDto {
    Create,
}

impl From<TagListAction> for TagListActionDto {
    fn from(action: TagListAction) -> Self {
        match action {
            TagListAction::Create => Self::Create,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = Tag)]
pub(crate) struct TagDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    pub allowed_actions: Vec<TagActionDto>,
}

impl From<TagView> for TagDto {
    fn from(view: TagView) -> Self {
        let tag = view.tag;
        Self {
            id: *tag.id.as_uuid(),
            heap_id: *tag.heap_id.as_uuid(),
            name: tag.name.as_str().to_owned(),
            description: tag.description.map(|d| d.as_str().to_owned()),
            version: tag.version.get(),
            allowed_actions: view.allowed_actions.into_iter().map(TagActionDto::from).collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = TagList)]
pub(crate) struct TagListDto {
    pub heap: HeapSummaryDto,
    /// By name.
    pub tags: Vec<TagDto>,
    pub allowed_actions: Vec<TagListActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = CreateTagRequest)]
pub(crate) struct CreateTagDto {
    /// 1 to 50 characters, unique in the heap ignoring case.
    pub name: String,
    pub description: Option<String>,
}

/// A partial edit. Leave a field out to keep it; send `null` to clear the description.
#[derive(Deserialize, ToSchema)]
#[schema(as = TagPatchRequest)]
pub(crate) struct TagPatchDto {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub description: Option<Option<String>>,
}

impl TagPatchDto {
    fn to_patch(&self) -> Result<TagPatch, ValidationError> {
        Ok(TagPatch {
            name: self.name.as_deref().map(TagName::parse).transpose()?,
            description: match &self.description {
                None => None,
                Some(None) => Some(None),
                Some(Some(raw)) => Some(TagDescription::parse(raw)?),
            },
        })
    }
}

/// The heap's tags, by name.
#[utoipa::path(get, path = "/heaps/{heap_id}/tags", tag = "tags", security(("session" = [])),
    params(("heap_id" = Uuid, Path)),
    responses((status = 200, body = TagListDto), (status = 404, body = Problem)))]
async fn list_tags(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
) -> Result<Json<TagListDto>, ApiError> {
    let list = state.services.list_tags(principal, HeapId::from_uuid(heap_id)).await?;
    Ok(Json(TagListDto {
        heap: HeapSummaryDto::from(&list.heap),
        tags: list.tags.into_iter().map(TagDto::from).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(TagListActionDto::from).collect(),
    }))
}

/// Adds a tag to the heap.
#[utoipa::path(post, path = "/heaps/{heap_id}/tags", tag = "tags", security(("session" = [])),
    params(("heap_id" = Uuid, Path)), request_body = CreateTagDto,
    responses(
        (status = 201, body = TagDto),
        (status = 404, body = Problem),
        (status = 409, description = "`tag_name_taken` or `heap_archived`", body = Problem),
        (status = 422, body = Problem),
    ))]
async fn create_tag(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(heap_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateTagDto>,
) -> Result<(StatusCode, Json<TagDto>), ApiError> {
    let fields = to_new_tag(&body).map_err(ServiceError::from)?;
    let view = state
        .services
        .create_tag(principal, HeapId::from_uuid(heap_id), fields)
        .await?;
    Ok((StatusCode::CREATED, Json(TagDto::from(view))))
}

/// Renames a tag or changes its description. Send the tag's `version` in `If-Match`.
#[utoipa::path(patch, path = "/tags/{tag_id}", tag = "tags", security(("session" = [])),
    params(("tag_id" = Uuid, Path), IfMatchHeader), request_body = TagPatchDto,
    responses(
        (status = 200, body = TagDto),
        (status = 404, body = Problem),
        (status = 409, description = "`tag_name_taken`, `version_conflict` or `heap_archived`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_tag(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiPath(tag_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TagPatchDto>,
) -> Result<Json<TagDto>, ApiError> {
    let patch = body.to_patch().map_err(ServiceError::from)?;
    let view = state
        .services
        .update_tag(principal, TagId::from_uuid(tag_id), patch, expected)
        .await?;
    Ok(Json(TagDto::from(view)))
}

fn to_new_tag(body: &CreateTagDto) -> Result<NewTag, ValidationError> {
    Ok(NewTag {
        name: TagName::parse(&body.name)?,
        description: body
            .description
            .as_deref()
            .map(TagDescription::parse)
            .transpose()?
            .flatten(),
    })
}
