//! A heap's custom parameters: listing them, and the admin's changes to them, their options and their tag links.
//! Parameters and options are archived, never deleted.
//!
//! Every endpoint returns the whole parameter, options and tag links included, so a client re-renders it from one
//! response. Changes take the parameter's `version` in `If-Match`; archiving says what the parameter or option
//! should end up as, so it takes none, like archiving a heap.

use std::collections::BTreeSet;

use api_types::common::HeapSummaryDto;
use api_types::parameters::{
    AddOptionDto, ArchiveOptionDto, ArchivedOptionDto, ArrangeOptionsDto, CreateParameterDto, LinkTagsDto, OptionDto,
    ParameterDto, ParameterListDto, ParameterPatchDto,
};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::parameter::{OptionLabel, Parameter, ParameterName, ParameterOption, ParameterPatch, Weight};
use domain::tag::parse_tag_set;
use domain::{OptionId, ParameterId, TagId, Timestamp, ValidationError};
use serde::Deserialize;
use service::ServiceError;
use service::parameters::{ParameterDraft, ParameterEntry};
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

pub(crate) fn to_parameter_dto(entry: ParameterEntry) -> ParameterDto {
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
    ParameterDto {
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
        parameters: list.parameters.into_iter().map(to_parameter_dto).collect(),
        archived: list.archived.into_iter().map(to_parameter_dto).collect(),
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
    Ok((StatusCode::CREATED, Json(to_parameter_dto(entry))))
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
    Ok(Json(to_parameter_dto(entry)))
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
    Ok(Json(to_parameter_dto(entry)))
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
    Ok(Json(to_parameter_dto(entry)))
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
    Ok(Json(to_parameter_dto(entry)))
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
    Ok(Json(to_parameter_dto(entry)))
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
    Ok(Json(to_parameter_dto(entry)))
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
