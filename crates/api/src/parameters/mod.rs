//! A heap's custom parameters: listing them, and the admin's changes to them, their options and their tag links.
//! Parameters and options are archived, never deleted.
//!
//! Every endpoint returns the whole parameter, options and tag links included, so a client re-renders it from one
//! response. Changes take the parameter's `version` in `If-Match`; archiving says what the parameter or option
//! should end up as, so it takes none, like archiving a heap.

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::parameter::{OptionLabel, Parameter, ParameterName, ParameterOption, ParameterPatch, Weight};
use domain::permission::{ParameterAction, ParameterListAction};
use domain::tag::parse_tag_set;
use domain::{OptionId, ParameterId, TagId, Timestamp, ValidationError};
use serde::{Deserialize, Serialize};
use service::ServiceError;
use service::parameters::{ParameterDraft, ParameterEntry};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{HeapSummaryDto, IfMatchHeader};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, IfMatch};
use crate::paths::{HeapParams, HeapPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_parameters, create_parameter))
        .routes(routes!(update_parameter))
        .routes(routes!(archive_parameter))
        .routes(routes!(add_option, arrange_options))
        .routes(routes!(archive_option))
        .routes(routes!(link_tags))
}

/// `{parameter_id}` in a path below the heap's.
#[derive(Deserialize)]
struct ParameterPathId {
    parameter_id: Uuid,
}

/// `{option_id}` in a path below the heap's.
#[derive(Deserialize)]
struct OptionPathId {
    option_id: Uuid,
}

/// An action the caller can take on a parameter.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ParameterAction)]
pub(crate) enum ParameterActionDto {
    /// Rename it, change its weight, change its options and link it to tags.
    Edit,
    Archive,
    ViewHistory,
}

impl From<ParameterAction> for ParameterActionDto {
    fn from(action: ParameterAction) -> Self {
        match action {
            ParameterAction::Edit => Self::Edit,
            ParameterAction::Archive => Self::Archive,
            ParameterAction::ViewHistory => Self::ViewHistory,
        }
    }
}

/// An action the caller can take on a heap's parameters.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ParameterListAction)]
pub(crate) enum ParameterListActionDto {
    Create,
}

impl From<ParameterListAction> for ParameterListActionDto {
    fn from(action: ParameterListAction) -> Self {
        match action {
            ParameterListAction::Create => Self::Create,
        }
    }
}

/// An option in use.
#[derive(Serialize, ToSchema)]
#[schema(as = ParameterOption)]
pub(crate) struct OptionDto {
    pub id: Uuid,
    pub label: String,
}

/// An archived option.
#[derive(Serialize, ToSchema)]
#[schema(as = ArchivedOption)]
pub(crate) struct ArchivedOptionDto {
    pub id: Uuid,
    pub label: String,
    /// UTC milliseconds.
    pub archived_at: i64,
    /// The option open items switched to. It may be archived too, with a replacement of its own.
    pub replaced_by_option_id: Uuid,
}

#[derive(Serialize, ToSchema)]
#[schema(as = Parameter)]
pub(crate) struct ParameterDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub name: String,
    /// Influence on the ranking next to the heap's other weights, 0 to 100.
    pub weight: u16,
    /// The options in use, highest priority first.
    pub options: Vec<OptionDto>,
    /// Archived options, in the order they were archived.
    pub archived_options: Vec<ArchivedOptionDto>,
    /// The tags whose items the parameter is suggested for.
    pub tag_ids: Vec<Uuid>,
    /// UTC milliseconds; `null` while the parameter is in use.
    pub archived_at: Option<i64>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    pub allowed_actions: Vec<ParameterActionDto>,
}

impl From<ParameterEntry> for ParameterDto {
    fn from(entry: ParameterEntry) -> Self {
        let Parameter {
            id,
            heap_id,
            name,
            weight,
            options,
            archived_options,
            tags,
            archived_at,
            version,
            ..
        } = entry.parameter;
        Self {
            id: *id.as_uuid(),
            heap_id: *heap_id.as_uuid(),
            name: name.as_str().to_owned(),
            weight: weight.get(),
            options: options
                .into_iter()
                .map(|o| OptionDto {
                    id: *o.id.as_uuid(),
                    label: o.label.as_str().to_owned(),
                })
                .collect(),
            archived_options: archived_options
                .into_iter()
                .map(|o| ArchivedOptionDto {
                    id: *o.id.as_uuid(),
                    label: o.label.as_str().to_owned(),
                    archived_at: o.archived_at.as_millis(),
                    replaced_by_option_id: *o.replaced_by.as_uuid(),
                })
                .collect(),
            tag_ids: tags.iter().map(|t| *t.as_uuid()).collect(),
            archived_at: archived_at.map(Timestamp::as_millis),
            version: version.get(),
            allowed_actions: entry.allowed_actions.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = ParameterList)]
pub(crate) struct ParameterListDto {
    pub heap: HeapSummaryDto,
    /// The parameters in use, by name.
    pub parameters: Vec<ParameterDto>,
    /// Archived parameters, by name. Closed items may still show their values.
    pub archived: Vec<ParameterDto>,
    pub allowed_actions: Vec<ParameterListActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CreateParameterRequest)]
pub(crate) struct CreateParameterDto {
    /// 1 to 50 characters, unique among the heap's parameters in use ignoring case.
    pub name: String,
    /// 0 to 100. Defaults to 10, the same as the heap's own weights.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: Option<i64>,
    /// Option labels, highest priority first: at most 50, unique ignoring case.
    #[serde(default)]
    pub options: Vec<String>,
    /// Tags of the same heap to suggest the parameter for.
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
}

/// A partial edit. Leave a field out to keep it.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ParameterPatchRequest)]
pub(crate) struct ParameterPatchDto {
    pub name: Option<String>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AddOptionRequest)]
pub(crate) struct AddOptionDto {
    /// 1 to 50 characters, unique among the parameter's options in use ignoring case. The new option comes last,
    /// as the lowest priority.
    pub label: String,
}

/// One option of a new arrangement.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OptionArrangement)]
pub(crate) struct OptionArrangementDto {
    pub id: Uuid,
    pub label: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ArrangeOptionsRequest)]
pub(crate) struct ArrangeOptionsDto {
    /// Every option in use, exactly once, in the new order (highest priority first) and with its label.
    pub options: Vec<OptionArrangementDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ArchiveOptionRequest)]
pub(crate) struct ArchiveOptionDto {
    /// Another option of the same parameter, in use. Open items with the archived option switch to it.
    pub replacement_option_id: Option<Uuid>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = LinkTagsRequest)]
pub(crate) struct LinkTagsDto {
    /// Tags of the same heap, at most 50. Replaces the parameter's tags; `[]` unlinks them all.
    pub tag_ids: Vec<Uuid>,
}

/// The heap's parameters. Any member may list them; only admins change them.
#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/parameters", tag = "parameters", security(("session" = [])),
    params(HeapParams),
    responses((status = 200, body = ParameterListDto), (status = 404, body = Problem)))]
async fn list_parameters(State(state): State<AppState>, path: HeapPath) -> Result<Json<ParameterListDto>, ApiError> {
    let list = state
        .services
        .list_parameters(path.principal, path.heap.heap_id)
        .await?;
    Ok(Json(ParameterListDto {
        heap: HeapSummaryDto::from(&list.heap),
        parameters: list.parameters.into_iter().map(ParameterDto::from).collect(),
        archived: list.archived.into_iter().map(ParameterDto::from).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(Into::into).collect(),
    }))
}

/// Adds a parameter to the heap. Admins only.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters", tag = "parameters", security(("session" = [])),
    params(HeapParams), request_body = CreateParameterDto,
    responses(
        (status = 201, body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_name_taken`, `option_label_taken` or `heap_archived`", body = Problem),
        (status = 422, description = "Invalid fields, `too_many_options`, `weight_out_of_range` or `tag_not_in_heap`", body = Problem),
    ))]
async fn create_parameter(
    State(state): State<AppState>,
    path: HeapPath,
    ApiJson(body): ApiJson<CreateParameterDto>,
) -> Result<(StatusCode, Json<ParameterDto>), ApiError> {
    let draft = to_draft(&body).map_err(|error| in_options(error.into()))?;
    let entry = state
        .services
        .create_parameter(path.principal, path.heap.heap_id, draft)
        .await?;
    Ok((StatusCode::CREATED, Json(ParameterDto::from(entry))))
}

/// Renames a parameter or changes its weight. Admins only; send the parameter's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = ParameterPatchDto,
    responses(
        (status = 200, body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_name_taken`, `parameter_archived`, `version_conflict` or `heap_archived`", body = Problem),
        (status = 422, description = "Invalid fields or `weight_out_of_range`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_parameter(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<ParameterPatchDto>,
) -> Result<Json<ParameterDto>, ApiError> {
    let patch = ParameterPatch {
        name: body
            .name
            .as_deref()
            .map(ParameterName::parse)
            .transpose()
            .map_err(ServiceError::from)?,
        weight: body.weight.map(Weight::parse).transpose().map_err(ServiceError::from)?,
    };
    let entry = state
        .services
        .update_parameter(
            path.principal,
            path.heap.heap_id,
            ParameterId::from_uuid(parameter_id),
            patch,
            expected,
        )
        .await?;
    Ok(Json(entry.into()))
}

/// Archives a parameter, once no open item has a value for it. Admins only. Archiving an archived parameter
/// succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/archive", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path)),
    responses(
        (status = 200, body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_in_use` or `heap_archived`", body = Problem),
    ))]
async fn archive_parameter(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
) -> Result<Json<ParameterDto>, ApiError> {
    let entry = state
        .services
        .archive_parameter(path.principal, path.heap.heap_id, ParameterId::from_uuid(parameter_id))
        .await?;
    Ok(Json(entry.into()))
}

/// Adds an option as the parameter's lowest priority. Admins only; send the parameter's `version` in `If-Match`.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/options", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = AddOptionDto,
    responses(
        (status = 200, body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`option_label_taken` (`field` `label`), `parameter_archived`, `version_conflict` or `heap_archived`", body = Problem),
        (status = 422, description = "An invalid label or `too_many_options`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn add_option(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<AddOptionDto>,
) -> Result<Json<ParameterDto>, ApiError> {
    let label = OptionLabel::parse(&body.label).map_err(ServiceError::from)?;
    let entry = state
        .services
        .add_option(
            path.principal,
            path.heap.heap_id,
            ParameterId::from_uuid(parameter_id),
            label,
            expected,
        )
        .await
        .map_err(|error| match error {
            ServiceError::Parameter(domain::parameter::ParameterError::OptionLabelTaken { .. }) => {
                ApiError::InField { error, field: "label" }
            }
            other => other.into(),
        })?;
    Ok(Json(entry.into()))
}

/// Puts the options in use in a new order, highest priority first, and relabels them. Admins only; send the
/// parameter's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/options", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = ArrangeOptionsDto,
    responses(
        (status = 200, body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`option_label_taken`, `parameter_archived`, `version_conflict` or `heap_archived`", body = Problem),
        (status = 422, description = "An invalid label, or `options_mismatch` when the list isn't every option in use exactly once", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn arrange_options(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<ArrangeOptionsDto>,
) -> Result<Json<ParameterDto>, ApiError> {
    let options = body
        .options
        .iter()
        .map(|option| {
            OptionLabel::parse(&option.label).map(|label| ParameterOption {
                id: OptionId::from_uuid(option.id),
                label,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| in_options(error.into()))?;
    let entry = state
        .services
        .arrange_options(
            path.principal,
            path.heap.heap_id,
            ParameterId::from_uuid(parameter_id),
            options,
            expected,
        )
        .await?;
    Ok(Json(entry.into()))
}

/// Archives an option. Open items with it switch to the replacement, and their versions go up; closed items keep
/// it. Admins only. Archiving an archived option succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/options/{option_id}/archive", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("option_id" = Uuid, Path)), request_body = ArchiveOptionDto,
    responses(
        (status = 200, description = "The option's parameter", body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_archived` or `heap_archived`", body = Problem),
        (status = 422, description = "`replacement_required`", body = Problem),
    ))]
async fn archive_option(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(OptionPathId { option_id }): ApiPath<OptionPathId>,
    ApiJson(body): ApiJson<ArchiveOptionDto>,
) -> Result<Json<ParameterDto>, ApiError> {
    let entry = state
        .services
        .archive_option(
            path.principal,
            path.heap.heap_id,
            OptionId::from_uuid(option_id),
            body.replacement_option_id.map(OptionId::from_uuid),
        )
        .await?;
    Ok(Json(entry.into()))
}

/// Sets the tags the parameter is suggested for. Admins only; send the parameter's `version` in `If-Match`.
#[utoipa::path(put, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/tags", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = LinkTagsDto,
    responses(
        (status = 200, body = ParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_archived`, `version_conflict` or `heap_archived`", body = Problem),
        (status = 422, description = "`tag_not_in_heap` or `too_many_tags`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn link_tags(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<LinkTagsDto>,
) -> Result<Json<ParameterDto>, ApiError> {
    let tags = to_tag_set(&body.tag_ids).map_err(ServiceError::from)?;
    let entry = state
        .services
        .link_parameter_tags(
            path.principal,
            path.heap.heap_id,
            ParameterId::from_uuid(parameter_id),
            tags,
            expected,
        )
        .await?;
    Ok(Json(entry.into()))
}

fn to_draft(body: &CreateParameterDto) -> Result<ParameterDraft, ValidationError> {
    Ok(ParameterDraft {
        name: ParameterName::parse(&body.name)?,
        weight: body.weight.map_or(Ok(Weight::DEFAULT), Weight::parse)?,
        options: body
            .options
            .iter()
            .map(|label| OptionLabel::parse(label))
            .collect::<Result<_, _>>()?,
        tags: to_tag_set(&body.tag_ids)?,
    })
}

fn to_tag_set(ids: &[Uuid]) -> Result<BTreeSet<TagId>, ValidationError> {
    parse_tag_set(ids.iter().copied().map(TagId::from_uuid))
}

/// An invalid label in a list of options is about `options`, not a `label` field.
fn in_options(error: ServiceError) -> ApiError {
    use domain::validation::Field;
    let label_error = matches!(
        &error,
        ServiceError::Invalid(
            ValidationError::Empty { field: Field::Label }
                | ValidationError::TooLong {
                    field: Field::Label,
                    ..
                }
        )
    );
    if label_error {
        return ApiError::InField {
            error,
            field: "options",
        };
    }
    error.into()
}
