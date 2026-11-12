//! Sending an item to an agent and taking it back. Both answer with the whole item.

use api_types::common::ItemDto;
use api_types::turns::SendToAgentDto;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::to_item_dto;
use crate::error::Problem;
use crate::extract::{ApiJson, ClientTimeZone, TimeZoneHeader};
use crate::paths::{ItemParams, ItemPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send_to_agent, take_back))
        .routes(routes!(retry_turn))
}

/// Sends an open item to an agent: opens a turn with a `reach` and an optional `line`, and answers with the whole
/// item. Any member may. The turn goes to the next stable state, `goal`, worked out from the item. It needs no
/// `If-Match`, and the item's `version` doesn't change. An item with an open question may be sent: the turn opens,
/// and the item waits as before. A blocked item can't be sent until it is unblocked.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/turn", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader), request_body = SendToAgentDto,
    responses(
        (status = 201, body = ItemDto),
        (status = 400, description = "`invalid_body`, such as an unknown field", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`, `item_closed`, `turn_item_blocked` (unblock the item first) or `turn_open` (an agent already has the item)", body = Problem),
        (status = 422, description = "Invalid fields: a `line` over 2,000 characters, or a `reach` this server doesn't know", body = Problem),
    ))]
async fn send_to_agent(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiJson(body): ApiJson<SendToAgentDto>,
) -> Result<(StatusCode, Json<ItemDto>), ApiError> {
    let (reach, line) = body.to_turn_fields().map_err(ServiceError::from)?;
    let view = state
        .services
        .send_to_agent(path.principal, path.item_id, reach, line)
        .await?;
    Ok((StatusCode::CREATED, Json(to_item_dto(view, state.services.now(), &tz))))
}

/// Takes an item back from the agent that has it and answers with the whole item. An item with no open turn is
/// answered unchanged and nothing is recorded, so two clients taking back at once both succeed. The item's
/// `version` doesn't change.
#[utoipa::path(delete, path = "/teams/{team}/heaps/{heap}/items/{item}/turn", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader),
    responses(
        (status = 200, body = ItemDto),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
    ))]
async fn take_back(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<ItemDto>, ApiError> {
    let view = state.services.take_back(path.principal, path.item_id).await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}

/// Lets the agents try again at an item whose turn is stuck (3 runs in a row failed) and answers with the whole item.
/// The count of failed runs goes back to 0, so the item is in the queue again, and `turn_retried` is recorded. Any
/// member who may edit items may. It needs no `If-Match`, and the item's `version` doesn't change. An item with no
/// open turn, or one that isn't stuck, is `turn_not_stuck`.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/turn/retry", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader),
    responses(
        (status = 200, body = ItemDto),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived` or `turn_not_stuck`", body = Problem),
    ))]
async fn retry_turn(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<ItemDto>, ApiError> {
    let view = state.services.retry_turn(path.principal, path.item_id).await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}
