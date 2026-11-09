//! Heaps: reading, renaming, archiving and unarchiving them. New heaps are created under their team
//! (`POST /teams/{team}/heaps`).

use api_types::heaps::{
    HeapActionDto, HeapDto, HeapPatchDto, HeapWeightsDto, HeapWeightsPatchDto, HeapWeightsResultDto,
};
use axum::Json;
use axum::extract::State;
use domain::handle::Handle;
use domain::heap::HeapPolicyPatch;
use domain::parameter::Weight;
use domain::text::Name;
use domain::validation::Field;
use domain::{ParameterId, Version};
use service::ServiceError;
use service::heaps::{HeapPatch, HeapWeightsView, ParameterWeightChange, WeightsChange};
use service::teams::HeapView;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, IfMatch};
use crate::parameters::to_parameter_dto;
use crate::paths::{HeapParams, HeapPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_heap, update_heap))
        .routes(routes!(update_weights))
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
        weights: HeapWeightsDto::from(&heap.policy),
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

fn to_weights_change(body: &HeapWeightsPatchDto) -> Result<WeightsChange, ServiceError> {
    let weight = |raw: Option<i64>| raw.map(Weight::parse).transpose();
    let parameters = body
        .parameters
        .iter()
        .map(|change| {
            Ok(ParameterWeightChange {
                id: ParameterId::from_uuid(change.id),
                weight: Weight::parse(change.weight)?,
                version: Version::new(change.version),
            })
        })
        .collect::<Result<_, ServiceError>>()?;
    Ok(WeightsChange {
        heap: HeapPolicyPatch {
            need: weight(body.need)?,
            age: weight(body.age)?,
            deadline: weight(body.deadline)?,
            value: weight(body.value)?,
        },
        parameters,
    })
}

fn to_weights_dto(view: HeapWeightsView) -> HeapWeightsResultDto {
    HeapWeightsResultDto {
        heap: to_heap_dto(view.heap),
        parameters: view.parameters.into_iter().map(to_parameter_dto).collect(),
    }
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

/// Sets the heap's weights and its parameters' weights in one transaction. Admins only; send the heap's `version` in
/// `If-Match`. The heap's version guards the whole save, even when only parameter weights change; each listed
/// parameter's `version` is checked too. Weights equal to the stored ones are skipped, and a request that changes
/// nothing succeeds. Any error saves nothing.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/weights", tag = "heaps", security(("session" = [])),
    params(HeapParams, IfMatchHeader), request_body = HeapWeightsPatchDto,
    responses(
        (status = 200, body = HeapWeightsResultDto),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `heap_archived` or `parameter_archived`", body = Problem),
        (status = 422, description = "Invalid fields, `weight_out_of_range`, `duplicate_parameter` or `parameter_not_in_heap`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_weights(
    State(state): State<AppState>,
    path: HeapPath,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<HeapWeightsPatchDto>,
) -> Result<Json<HeapWeightsResultDto>, ApiError> {
    let view = state
        .services
        .update_weights(path.principal, path.heap.heap_id, to_weights_change(&body)?, expected)
        .await?;
    Ok(Json(to_weights_dto(view)))
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
