//! A heap's custom parameters and its team's: listing them, and the admin's changes to them, their options and their
//! tag links, and which of the team's a heap uses. Parameters and options are archived, never deleted.
//!
//! Every endpoint returns the whole parameter, options and tag links included, so a client re-renders it from one
//! response. Changes take the parameter's `version` in `If-Match`; archiving says what the parameter or option
//! should end up as, so it takes none, like archiving a heap.

use std::collections::BTreeSet;

use api_types::common::HeapSummaryDto;
use api_types::parameters::{
    AddOptionDto, ArchiveOptionDto, ArchivedOptionDto, ArrangeOptionsDto, ChangeHeapTagLinksDto, CreateParameterDto,
    CreateTeamParameterDto, LinkTagsDto, OptionDto, ParameterDto, ParameterKindDto, ParameterListDto,
    ParameterPatchDto, TeamParameterDto, TeamParameterListDto, TeamParameterPatchDto,
};
use api_types::problem::{ParameterMergeReachDto, ParameterMergeReachEntryDto, ParameterMergeSwitchDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::parameter::{
    OptionLabel, Parameter, ParameterName, ParameterOption, ParameterOwner, ParameterPatch, Weight,
};
use domain::tag::parse_tag_set;
use domain::{HeapId, OptionId, ParameterId, TagId, Timestamp, ValidationError};
use serde::Deserialize;
use service::ServiceError;
use service::parameters::{
    ParameterDraft, ParameterEntry, ParameterMergeReach, TeamParameterDraft, TeamParameterEntry,
};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, ApiQuery, IfMatch};
use crate::paths::{HeapParams, HeapPath, TeamParams, TeamPath};
use crate::tags::ConfirmReachQuery;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_parameters, create_parameter))
        .routes(routes!(update_parameter))
        .routes(routes!(archive_parameter))
        .routes(routes!(make_team_parameter))
        .routes(routes!(add_option, arrange_options))
        .routes(routes!(archive_option))
        .routes(routes!(link_tags))
        .routes(routes!(link_heap_tags))
        .routes(routes!(use_team_parameter))
        .routes(routes!(stop_using_team_parameter))
        .routes(routes!(list_team_parameters, create_team_parameter))
        .routes(routes!(update_team_parameter))
        .routes(routes!(archive_team_parameter))
        .routes(routes!(link_team_parameter_tags))
        .routes(routes!(add_team_option, arrange_team_options))
        .routes(routes!(archive_team_option))
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

pub(crate) fn to_parameter_merge_reach_dto(reach: &ParameterMergeReach) -> ParameterMergeReachDto {
    ParameterMergeReachDto {
        token: reach.token.clone(),
        entries: reach
            .entries
            .iter()
            .map(|entry| ParameterMergeReachEntryDto {
                heap: HeapSummaryDto::from(&entry.heap),
                archived: entry.heap.is_archived(),
                parameter_id: *entry.parameter.id.as_uuid(),
                parameter_name: entry.parameter.name.as_str().to_owned(),
                items: entry.items,
                added_options: entry.added.iter().map(|label| label.as_str().to_owned()).collect(),
                switched: entry
                    .switched
                    .iter()
                    .map(|switch| ParameterMergeSwitchDto {
                        from: switch.from.as_str().to_owned(),
                        to: switch.to.as_str().to_owned(),
                    })
                    .collect(),
                linked_tags: entry.linked_tags.clone(),
                starting: entry.starting,
            })
            .collect(),
    }
}

/// The parameter as `heap_id` lists it.
pub(crate) fn to_parameter_dto(entry: ParameterEntry, heap_id: HeapId) -> ParameterDto {
    let suggesting = entry.parameter.suggesting_tags();
    let Parameter {
        id,
        owner,
        name,
        weight,
        options,
        archived_options,
        tags,
        heap_tags,
        archived_at,
        version,
        ..
    } = entry.parameter;
    let kind = match owner {
        ParameterOwner::Heap(_) => ParameterKindDto::Heap,
        ParameterOwner::Team(_) => ParameterKindDto::Team,
    };
    let uuids = |tags: &BTreeSet<TagId>| tags.iter().map(|t| *t.as_uuid()).collect::<Vec<_>>();
    let team_tags = if kind == ParameterKindDto::Team {
        uuids(&tags)
    } else {
        Vec::new()
    };
    ParameterDto {
        id: *id.as_uuid(),
        kind,
        heap_id: *heap_id.as_uuid(),
        name: name.as_str().to_owned(),
        weight: weight.get(),
        options: to_option_dtos(options),
        archived_options: to_archived_option_dtos(archived_options),
        tag_ids: uuids(&suggesting),
        team_tag_ids: team_tags,
        heap_tag_ids: uuids(&heap_tags),
        archived_at: archived_at.map(Timestamp::as_millis),
        version: version.get(),
        allowed_actions: entry.allowed_actions.into_iter().map(Into::into).collect(),
    }
}

fn to_option_dtos(options: Vec<ParameterOption>) -> Vec<OptionDto> {
    options
        .into_iter()
        .map(|o| OptionDto {
            id: *o.id.as_uuid(),
            label: o.label.as_str().to_owned(),
        })
        .collect()
}

fn to_archived_option_dtos(options: Vec<domain::parameter::ArchivedOption>) -> Vec<ArchivedOptionDto> {
    options
        .into_iter()
        .map(|o| ArchivedOptionDto {
            id: *o.id.as_uuid(),
            label: o.label.as_str().to_owned(),
            archived_at: o.archived_at.as_millis(),
            replaced_by_option_id: *o.replaced_by.as_uuid(),
        })
        .collect()
}

fn to_team_parameter_dto(entry: TeamParameterEntry) -> TeamParameterDto {
    let Parameter {
        id,
        name,
        options,
        archived_options,
        tags,
        archived_at,
        version,
        ..
    } = entry.parameter;
    TeamParameterDto {
        id: *id.as_uuid(),
        team_id: *entry.team_id.as_uuid(),
        name: name.as_str().to_owned(),
        options: to_option_dtos(options),
        archived_options: to_archived_option_dtos(archived_options),
        tag_ids: tags.iter().map(|t| *t.as_uuid()).collect(),
        archived_at: archived_at.map(Timestamp::as_millis),
        version: version.get(),
        heaps: entry.heaps.iter().map(HeapSummaryDto::from).collect(),
        allowed_actions: entry.allowed_actions.into_iter().map(Into::into).collect(),
    }
}

fn to_parameter_list_dto(list: service::parameters::ParameterList) -> ParameterListDto {
    let heap_id = list.heap.id;
    ParameterListDto {
        heap: HeapSummaryDto::from(&list.heap),
        parameters: list
            .parameters
            .into_iter()
            .map(|e| to_parameter_dto(e, heap_id))
            .collect(),
        archived: list
            .archived
            .into_iter()
            .map(|e| to_parameter_dto(e, heap_id))
            .collect(),
        team_available: list.team_available.into_iter().map(to_team_parameter_dto).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(Into::into).collect(),
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
    Ok(Json(to_parameter_list_dto(list)))
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
    Ok((StatusCode::CREATED, Json(to_parameter_dto(entry, path.heap.heap_id))))
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
    Ok(Json(to_parameter_dto(entry, path.heap.heap_id)))
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
    Ok(Json(to_parameter_dto(entry, path.heap.heap_id)))
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
        .map_err(label_taken_in_label)?;
    Ok(Json(to_parameter_dto(entry, path.heap.heap_id)))
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
    let options = to_arranged_options(&body)?;
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
    Ok(Json(to_parameter_dto(entry, path.heap.heap_id)))
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
    Ok(Json(to_parameter_dto(entry, path.heap.heap_id)))
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
    Ok(Json(to_parameter_dto(entry, path.heap.heap_id)))
}

/// Makes a heap parameter its team's, in place, and merges the team's same-named heap parameters into it. Admins
/// only. It can't be undone, so it needs `confirm_reach`, the `token` of the `parameter_merge_reach` of a
/// `parameter_merge_unconfirmed` refusal, even when nothing merges. Every heap that had one of them uses the team
/// parameter at the weight it had, and each item's value moves to the option with the same label; for an open item
/// whose option the team parameter has archived, that is the option in use it was replaced by (`switched`). The tags
/// a parameter is linked to become its heap's links to the team parameter, for that heap only (`linked_tags`).
/// Repeating the write on a team parameter the heap uses succeeds and changes nothing.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/team", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path), ConfirmReachQuery),
    responses(
        (status = 200, description = "The team parameter, with the heaps that use it", body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, description = "Not a parameter of the heap", body = Problem),
        (status = 409, description = "`parameter_merge_unconfirmed`, `parameter_archived` or `heap_archived`", body = Problem),
        (status = 422, description = "`too_many_options`", body = Problem),
    ))]
async fn make_team_parameter(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiQuery(query): ApiQuery<ConfirmReachQuery>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let entry = state
        .services
        .make_team_parameter(
            path.principal,
            path.heap.heap_id,
            ParameterId::from_uuid(parameter_id),
            query.confirm_reach.as_deref(),
        )
        .await?;
    Ok(Json(to_team_parameter_dto(entry)))
}

/// Links tags of the heap (its own and its team's) to a team parameter the heap uses, and unlinks others, so the
/// parameter is suggested for the linked ones in this heap only, besides the team's links. Admins only. Each tag
/// named ends up linked or unlinked, and the others are left alone, so two writes that race don't undo each other:
/// it takes no `If-Match`, as `use` and `stop-using` take none. Linking a tag linked already, or unlinking one not
/// linked, succeeds; a change that changes nothing writes nothing.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/heap-tags", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path)), request_body = ChangeHeapTagLinksDto,
    responses(
        (status = 200, body = ParameterListDto),
        (status = 403, body = Problem),
        (status = 404, description = "Not a team parameter the heap uses", body = Problem),
        (status = 409, description = "`parameter_archived` or `heap_archived`", body = Problem),
        (status = 422, description = "`tag_not_in_heap`, `tag_linked_and_unlinked` or `too_many_tags` (more than 50 linked once done), each with `field` `link`", body = Problem),
    ))]
async fn link_heap_tags(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<ChangeHeapTagLinksDto>,
) -> Result<Json<ParameterListDto>, ApiError> {
    let link = to_tag_set(&body.link).map_err(|error| in_link(error.into()))?;
    let unlink = body.unlink.iter().copied().map(TagId::from_uuid).collect();
    let list = state
        .services
        .link_heap_tags(
            path.principal,
            path.heap.heap_id,
            ParameterId::from_uuid(parameter_id),
            link,
            unlink,
        )
        .await
        .map_err(in_link)?;
    Ok(Json(to_parameter_list_dto(list)))
}

/// A tag that can't be linked, or too many linked, is about `link`: only the tags to link are checked.
fn in_link(error: ServiceError) -> ApiError {
    match error {
        ServiceError::TagNotInHeap { .. } | ServiceError::Invalid(ValidationError::TooManyTags { .. }) => {
            ApiError::InField { error, field: "link" }
        }
        other => other.into(),
    }
}

/// Makes the heap use one of its team's parameters. Admins only. The first use gives it the default weight, and
/// using it again after stopping restores the weight it had. Using one that is used already succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/use", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path)),
    responses(
        (status = 200, body = ParameterListDto),
        (status = 403, body = Problem),
        (status = 404, description = "Not a parameter of the heap's team", body = Problem),
        (status = 409, description = "`parameter_archived` or `heap_archived`", body = Problem),
    ))]
async fn use_team_parameter(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
) -> Result<Json<ParameterListDto>, ApiError> {
    let list = state
        .services
        .use_team_parameter(path.principal, path.heap.heap_id, ParameterId::from_uuid(parameter_id))
        .await?;
    Ok(Json(to_parameter_list_dto(list)))
}

/// Makes the heap stop using a team parameter, once no open item of the heap has a value for it. Admins only. Its
/// weight is kept. Stopping one the heap doesn't use succeeds.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/stop-using", tag = "parameters", security(("session" = [])),
    params(HeapParams, ("parameter_id" = Uuid, Path)),
    responses(
        (status = 200, body = ParameterListDto),
        (status = 403, body = Problem),
        (status = 404, description = "Not a parameter of the heap's team", body = Problem),
        (status = 409, description = "`parameter_in_use` or `heap_archived`", body = Problem),
    ))]
async fn stop_using_team_parameter(
    State(state): State<AppState>,
    path: HeapPath,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
) -> Result<Json<ParameterListDto>, ApiError> {
    let list = state
        .services
        .stop_using_team_parameter(path.principal, path.heap.heap_id, ParameterId::from_uuid(parameter_id))
        .await?;
    Ok(Json(to_parameter_list_dto(list)))
}

/// The team's parameters, by name. Any member may list them; the heaps of the team choose which to use.
#[utoipa::path(get, path = "/teams/{team}/parameters", tag = "parameters", security(("session" = [])),
    params(TeamParams),
    responses((status = 200, body = TeamParameterListDto), (status = 404, body = Problem)))]
async fn list_team_parameters(
    State(state): State<AppState>,
    path: TeamPath,
) -> Result<Json<TeamParameterListDto>, ApiError> {
    let list = state
        .services
        .list_team_parameters(path.principal, path.team_id)
        .await?;
    Ok(Json(TeamParameterListDto {
        parameters: list.parameters.into_iter().map(to_team_parameter_dto).collect(),
        archived: list.archived.into_iter().map(to_team_parameter_dto).collect(),
        allowed_actions: list.allowed_actions.into_iter().map(Into::into).collect(),
    }))
}

/// Adds a parameter to the team, for its heaps to use. Admins only. The name must be free among the parameters in
/// use of the team and of all its heaps.
#[utoipa::path(post, path = "/teams/{team}/parameters", tag = "parameters", security(("session" = [])),
    params(TeamParams), request_body = CreateTeamParameterDto,
    responses(
        (status = 201, body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_name_taken` or `option_label_taken`", body = Problem),
        (status = 422, description = "Invalid fields or `too_many_options`", body = Problem),
    ))]
async fn create_team_parameter(
    State(state): State<AppState>,
    path: TeamPath,
    ApiJson(body): ApiJson<CreateTeamParameterDto>,
) -> Result<(StatusCode, Json<TeamParameterDto>), ApiError> {
    let draft = to_team_draft(&body).map_err(|error| in_options(error.into()))?;
    let entry = state
        .services
        .create_team_parameter(path.principal, path.team_id, draft)
        .await?;
    Ok((StatusCode::CREATED, Json(to_team_parameter_dto(entry))))
}

/// Renames a team's parameter, in every heap that uses it. Admins only; send the parameter's `version` in
/// `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/parameters/{parameter_id}", tag = "parameters", security(("session" = [])),
    params(TeamParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = TeamParameterPatchDto,
    responses(
        (status = 200, body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_name_taken`, `parameter_archived` or `version_conflict`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_team_parameter(
    State(state): State<AppState>,
    path: TeamPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<TeamParameterPatchDto>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let name = ParameterName::parse(&body.name).map_err(ServiceError::from)?;
    let entry = state
        .services
        .update_team_parameter(
            path.principal,
            path.team_id,
            ParameterId::from_uuid(parameter_id),
            name,
            expected,
        )
        .await?;
    Ok(Json(to_team_parameter_dto(entry)))
}

/// Archives a team's parameter, once no open item in any heap of the team has a value for it. Admins only.
/// Archiving an archived parameter succeeds.
#[utoipa::path(post, path = "/teams/{team}/parameters/{parameter_id}/archive", tag = "parameters", security(("session" = [])),
    params(TeamParams, ("parameter_id" = Uuid, Path)),
    responses(
        (status = 200, body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_in_use`", body = Problem),
    ))]
async fn archive_team_parameter(
    State(state): State<AppState>,
    path: TeamPath,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let entry = state
        .services
        .archive_team_parameter(path.principal, path.team_id, ParameterId::from_uuid(parameter_id))
        .await?;
    Ok(Json(to_team_parameter_dto(entry)))
}

/// Sets the team's tags a team parameter is suggested for, in every heap that uses it. Admins only; send the
/// parameter's `version` in `If-Match`.
#[utoipa::path(put, path = "/teams/{team}/parameters/{parameter_id}/tags", tag = "parameters", security(("session" = [])),
    params(TeamParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = LinkTagsDto,
    responses(
        (status = 200, body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_archived` or `version_conflict`", body = Problem),
        (status = 422, description = "`tag_not_in_team` or `too_many_tags`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn link_team_parameter_tags(
    State(state): State<AppState>,
    path: TeamPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<LinkTagsDto>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let tags = to_tag_set(&body.tag_ids).map_err(ServiceError::from)?;
    let entry = state
        .services
        .link_team_parameter_tags(
            path.principal,
            path.team_id,
            ParameterId::from_uuid(parameter_id),
            tags,
            expected,
        )
        .await?;
    Ok(Json(to_team_parameter_dto(entry)))
}

/// Adds an option to a team's parameter as its lowest priority. Admins only; send the parameter's `version` in
/// `If-Match`.
#[utoipa::path(post, path = "/teams/{team}/parameters/{parameter_id}/options", tag = "parameters", security(("session" = [])),
    params(TeamParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = AddOptionDto,
    responses(
        (status = 200, body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`option_label_taken` (`field` `label`), `parameter_archived` or `version_conflict`", body = Problem),
        (status = 422, description = "An invalid label or `too_many_options`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn add_team_option(
    State(state): State<AppState>,
    path: TeamPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<AddOptionDto>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let label = OptionLabel::parse(&body.label).map_err(ServiceError::from)?;
    let entry = state
        .services
        .add_team_option(
            path.principal,
            path.team_id,
            ParameterId::from_uuid(parameter_id),
            label,
            expected,
        )
        .await
        .map_err(label_taken_in_label)?;
    Ok(Json(to_team_parameter_dto(entry)))
}

/// Puts a team parameter's options in use in a new order, highest priority first, and relabels them. Admins only;
/// send the parameter's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/parameters/{parameter_id}/options", tag = "parameters", security(("session" = [])),
    params(TeamParams, ("parameter_id" = Uuid, Path), IfMatchHeader), request_body = ArrangeOptionsDto,
    responses(
        (status = 200, body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`option_label_taken`, `parameter_archived` or `version_conflict`", body = Problem),
        (status = 422, description = "An invalid label, or `options_mismatch` when the list isn't every option in use exactly once", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn arrange_team_options(
    State(state): State<AppState>,
    path: TeamPath,
    IfMatch(expected): IfMatch,
    ApiPath(ParameterPathId { parameter_id }): ApiPath<ParameterPathId>,
    ApiJson(body): ApiJson<ArrangeOptionsDto>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let options = to_arranged_options(&body)?;
    let entry = state
        .services
        .arrange_team_options(
            path.principal,
            path.team_id,
            ParameterId::from_uuid(parameter_id),
            options,
            expected,
        )
        .await?;
    Ok(Json(to_team_parameter_dto(entry)))
}

/// Archives an option of a team's parameter. Open items with it, in every heap of the team, switch to the
/// replacement, and their versions go up; closed items keep it. Admins only. Archiving an archived option succeeds.
#[utoipa::path(post, path = "/teams/{team}/options/{option_id}/archive", tag = "parameters", security(("session" = [])),
    params(TeamParams, ("option_id" = Uuid, Path)), request_body = ArchiveOptionDto,
    responses(
        (status = 200, description = "The option's parameter", body = TeamParameterDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_archived`", body = Problem),
        (status = 422, description = "`replacement_required`", body = Problem),
    ))]
async fn archive_team_option(
    State(state): State<AppState>,
    path: TeamPath,
    ApiPath(OptionPathId { option_id }): ApiPath<OptionPathId>,
    ApiJson(body): ApiJson<ArchiveOptionDto>,
) -> Result<Json<TeamParameterDto>, ApiError> {
    let entry = state
        .services
        .archive_team_option(
            path.principal,
            path.team_id,
            OptionId::from_uuid(option_id),
            body.replacement_option_id.map(OptionId::from_uuid),
        )
        .await?;
    Ok(Json(to_team_parameter_dto(entry)))
}

fn to_team_draft(body: &CreateTeamParameterDto) -> Result<TeamParameterDraft, ValidationError> {
    Ok(TeamParameterDraft {
        name: ParameterName::parse(&body.name)?,
        options: body
            .options
            .iter()
            .map(|label| OptionLabel::parse(label))
            .collect::<Result<_, _>>()?,
    })
}

fn to_arranged_options(body: &ArrangeOptionsDto) -> Result<Vec<ParameterOption>, ApiError> {
    body.options
        .iter()
        .map(|option| {
            OptionLabel::parse(&option.label).map(|label| ParameterOption {
                id: OptionId::from_uuid(option.id),
                label,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| in_options(error.into()))
}

/// A taken label is about the `label` field of the request that added it.
fn label_taken_in_label(error: ServiceError) -> ApiError {
    match error {
        ServiceError::Parameter(domain::parameter::ParameterError::OptionLabelTaken { .. }) => {
            ApiError::InField { error, field: "label" }
        }
        other => other.into(),
    }
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
