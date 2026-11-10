//! Adding a step to a task and editing one. Both answer with the whole item.

use api_types::common::ItemDto;
use api_types::steps::{AddStepDto, StepPatchDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::{IfMatchHeader, to_item_dto};
use crate::error::Problem;
use crate::extract::{ApiJson, ClientTimeZone, IfMatch, TimeZoneHeader};
use crate::paths::{ItemParams, ItemPath, StepParams, StepPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(add_step))
        .routes(routes!(update_step))
}

/// Adds a step to a task, at the end or right after the step numbered `after`, and answers with the whole item. The
/// new step is the one with the highest `number`. A task has at most 100 steps, dropped ones included. Adding needs
/// no `If-Match`; a step has its own version, and the item's `version` doesn't change.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/steps", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader), request_body = AddStepDto,
    responses(
        (status = 201, body = ItemDto),
        (status = 400, description = "`invalid_body`, such as an unknown field", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`, `item_closed`, `steps_need_task` or `too_many_steps`", body = Problem),
        (status = 422, description = "Invalid fields, `value_unknown` (an `after` of 0) or `step_not_in_item` (`field` `after`)", body = Problem),
    ))]
async fn add_step(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiJson(body): ApiJson<AddStepDto>,
) -> Result<(StatusCode, Json<ItemDto>), ApiError> {
    let fields = body.to_new_step().map_err(ServiceError::from)?;
    let view = state.services.add_step(path.principal, path.item_id, fields).await?;
    Ok((StatusCode::CREATED, Json(to_item_dto(view, state.services.now(), &tz))))
}

/// Edits a step of a task and answers with the whole item. `{step}` is the step's number. Send the step's `version`
/// in `If-Match`; the item's `version` doesn't change. A step can move from any state to any other.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/items/{item}/steps/{step}", tag = "items", security(("session" = [])),
    params(StepParams, IfMatchHeader, TimeZoneHeader), request_body = StepPatchDto,
    responses(
        (status = 200, body = ItemDto),
        (status = 400, description = "`invalid_body`, such as an unknown field", body = Problem),
        (status = 404, description = "No such item, or no step with that number", body = Problem),
        (status = 409, description = "`version_conflict`, `item_closed` or `heap_archived`", body = Problem),
        (status = 422, description = "Invalid fields or `value_unknown` (a `state` the server doesn't know)", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_step(
    State(state): State<AppState>,
    path: StepPath,
    ClientTimeZone(tz): ClientTimeZone,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<StepPatchDto>,
) -> Result<Json<ItemDto>, ApiError> {
    let patch = body.to_patch().map_err(ServiceError::from)?;
    let view = state
        .services
        .update_step(path.principal, path.item_id, path.number, patch, expected)
        .await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}
