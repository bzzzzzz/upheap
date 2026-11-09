//! The ranked list, closed items, and adding and editing items.

use api_types::common::{HeapSummaryDto, ItemDto};
use api_types::items::{ClosedItemsDto, ClosedQuery, CreateItemDto, ItemPatchDto, RankedListDto, RankedQuery};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::item::{ItemPatch, StateKind};
use domain::parameter::ParameterError;
use domain::{ItemId, Timestamp, ValidationError};
use service::ServiceError;
use service::items::ClosedCursor;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{IfMatchHeader, to_item_dto, to_ranked_dto};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiQuery, ClientTimeZone, IfMatch, TimeZoneHeader};
use crate::paths::{HeapParams, HeapPath, ItemParams, ItemPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_ranked, create_item))
        .routes(routes!(list_closed))
        .routes(routes!(get_item, update_item))
        .routes(routes!(reopen_item))
}

/// The heap's non-closed items, ranked, with the reasons for each position. Ranking runs over every non-closed
/// item; the view then picks, groups and sorts what is returned, so filtered items keep their relative order.
#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/items", tag = "items", security(("session" = [])),
    params(HeapParams, RankedQuery, TimeZoneHeader),
    responses((status = 200, body = RankedListDto), (status = 400, description = "`invalid_query`", body = Problem), (status = 404, body = Problem), (status = 422, description = "`tag_not_in_heap` (`field` `tag`), `option_not_in_heap` (`param`), `parameter_not_in_heap` or `sort_parameter_mismatch` (`sort_parameter`)", body = Problem)))]
async fn list_ranked(
    State(state): State<AppState>,
    path: HeapPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<RankedQuery>,
) -> Result<Json<RankedListDto>, ApiError> {
    let as_of = query
        .as_of
        .map(|ms| Timestamp::from_millis(ms).ok_or(ValidationError::TimestampOutOfRange))
        .transpose()
        .map_err(ServiceError::from)?;
    let view = query.to_view().map_err(ServiceError::from)?;
    let list = state
        .services
        .list_ranked(path.principal, path.heap.heap_id, as_of, &view)
        .await
        .map_err(|error| match error {
            ServiceError::TagNotInHeap { .. } => ApiError::InField { error, field: "tag" },
            ServiceError::Parameter(ParameterError::NotInHeap { .. }) => ApiError::InField {
                error,
                field: "sort_parameter",
            },
            other => other.into(),
        })?;
    let as_of = list.as_of;
    let group = |entries: Vec<_>| entries.into_iter().map(|e| to_ranked_dto(e, as_of, &tz)).collect();
    Ok(Json(RankedListDto {
        heap: HeapSummaryDto::from(&list.heap),
        as_of: as_of.as_millis(),
        pinned: group(list.pinned),
        ranked: group(list.ranked),
        blocked: group(list.blocked),
        hidden_count: list.hidden_count,
        unfiltered_count: list.unfiltered_count,
        in_progress_count: list.in_progress_count,
    }))
}

/// Closed items, most recently closed first.
#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/items/closed", tag = "items", security(("session" = [])),
    params(HeapParams, ClosedQuery, TimeZoneHeader),
    responses((status = 200, body = ClosedItemsDto), (status = 404, body = Problem)))]
async fn list_closed(
    State(state): State<AppState>,
    path: HeapPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<ClosedQuery>,
) -> Result<Json<ClosedItemsDto>, ApiError> {
    let cursor = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let page = state
        .services
        .list_closed(path.principal, path.heap.heap_id, cursor, query.limit.unwrap_or(50))
        .await?;
    let now = state.services.now();
    Ok(Json(ClosedItemsDto {
        heap: HeapSummaryDto::from(&page.heap),
        items: page.items.into_iter().map(|view| to_item_dto(view, now, &tz)).collect(),
        next_cursor: page.next.map(|c| format!("{}.{}", c.closed_at.as_millis(), c.id)),
    }))
}

/// Adds an item. Only the title is required.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items", tag = "items", security(("session" = [])),
    params(HeapParams, TimeZoneHeader), request_body = CreateItemDto,
    responses((status = 201, body = ItemDto), (status = 404, body = Problem), (status = 409, description = "`heap_archived`", body = Problem), (status = 422, description = "Invalid fields, `tag_not_in_heap`, `parameter_not_in_heap` or `option_not_in_parameter`", body = Problem)))]
async fn create_item(
    State(state): State<AppState>,
    path: HeapPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiJson(body): ApiJson<CreateItemDto>,
) -> Result<(StatusCode, Json<ItemDto>), ApiError> {
    let fields = body.to_new_item(&tz).map_err(ServiceError::from)?;
    let view = state
        .services
        .create_item(path.principal, path.heap.heap_id, fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_item_dto(view, state.services.now(), &tz))))
}

#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/items/{item}", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader),
    responses((status = 200, body = ItemDto), (status = 404, body = Problem)))]
async fn get_item(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<ItemDto>, ApiError> {
    let view = state.services.find_item(path.principal, path.item_id).await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}

/// Edits an item. Send the item's `version` in `If-Match`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/items/{item}", tag = "items", security(("session" = [])),
    params(ItemParams, IfMatchHeader, TimeZoneHeader), request_body = ItemPatchDto,
    responses(
        (status = 200, body = ItemDto),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `item_closed`, `reopen_window_expired` or `heap_archived`", body = Problem),
        (status = 422, description = "Invalid fields, `tag_not_in_heap`, `parameter_not_in_heap`, `option_not_in_parameter`, `item_not_blocked`, `blocker_not_in_heap`, `blocker_closed` or `blocking_cycle`", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_item(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<ItemPatchDto>,
) -> Result<Json<ItemDto>, ApiError> {
    let patch = body.to_patch(&tz).map_err(ServiceError::from)?;
    let view = state
        .services
        .update_item(path.principal, path.item_id, patch, expected)
        .await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}

/// Reopens a closed item, within a week of closing it.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/reopen", tag = "items", security(("session" = [])),
    params(ItemParams, IfMatchHeader, TimeZoneHeader),
    responses(
        (status = 200, body = ItemDto),
        (status = 404, body = Problem),
        (status = 409, description = "`reopen_window_expired` or `version_conflict`", body = Problem),
    ))]
async fn reopen_item(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    IfMatch(expected): IfMatch,
) -> Result<Json<ItemDto>, ApiError> {
    let patch = ItemPatch {
        state: Some(StateKind::Open),
        ..ItemPatch::default()
    };
    let view = state
        .services
        .update_item(path.principal, path.item_id, patch, expected)
        .await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}

fn parse_cursor(raw: &str) -> Result<ClosedCursor, ApiError> {
    let invalid = || ApiError::BadRequest {
        code: "invalid_cursor",
        detail: "cursor is not one this server issued".to_owned(),
    };
    let (millis, id) = raw.split_once('.').ok_or_else(invalid)?;
    let closed_at = millis
        .parse()
        .ok()
        .and_then(Timestamp::from_millis)
        .ok_or_else(invalid)?;
    let id = Uuid::parse_str(id).map_err(|_| invalid())?;
    Ok(ClosedCursor {
        closed_at,
        id: ItemId::from_uuid(id),
    })
}
