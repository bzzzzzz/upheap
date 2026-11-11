//! The ranked list, closed items, and adding and editing items.

use api_types::common::{HeapSummaryDto, ItemDto, ItemRefDto};
use api_types::items::{
    ClosedItemsDto, ClosedQuery, CreateItemDto, ItemMoveDto, ItemPatchDto, RankedListDto, RankedQuery,
    RankingPreviewDto, RankingPreviewItemDto, RankingPreviewRequestDto,
};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::heap::HeapPolicyPatch;
use domain::item::{ItemPatch, StateKind};
use domain::parameter::{ParameterError, Weight};
use domain::{HeapId, ItemId, ParameterId, Timestamp, ValidationError};
use service::ServiceError;
use service::items::{ClosedCursor, ParameterWeightDraft, WeightsDraft, parse_preview_limit};
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
        .routes(routes!(preview_ranking))
        .routes(routes!(list_closed))
        .routes(routes!(get_item, update_item))
        .routes(routes!(reopen_item))
        .routes(routes!(move_item))
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

/// Ranks the heap's items with draft weights and returns the top of the ranked group, each with its position now.
/// Nothing is saved and no audit entry is made; this is a `POST` only because it carries a body. Any member may ask,
/// and an archived heap can be previewed. Weights left out keep their stored values.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/ranking/preview", tag = "items", security(("session" = [])),
    params(HeapParams), request_body = RankingPreviewRequestDto,
    responses(
        (status = 200, body = RankingPreviewDto),
        (status = 404, body = Problem),
        (status = 409, description = "`parameter_archived`", body = Problem),
        (status = 422, description = "Invalid fields, `weight_out_of_range`, `limit_out_of_range`, `duplicate_parameter` or `parameter_not_in_heap`", body = Problem),
    ))]
async fn preview_ranking(
    State(state): State<AppState>,
    path: HeapPath,
    ApiJson(body): ApiJson<RankingPreviewRequestDto>,
) -> Result<Json<RankingPreviewDto>, ApiError> {
    let limit = parse_preview_limit(body.limit).map_err(ServiceError::from)?;
    let draft = to_weights_draft(&body).map_err(ServiceError::from)?;
    let preview = state
        .services
        .preview_ranking(path.principal, path.heap.heap_id, draft, limit)
        .await?;
    Ok(Json(RankingPreviewDto {
        as_of: preview.as_of.as_millis(),
        items: preview
            .items
            .into_iter()
            .map(|entry| RankingPreviewItemDto {
                item: ItemRefDto::from(entry.item),
                position: entry.position,
                current_position: entry.current_position,
            })
            .collect(),
    }))
}

fn to_weights_draft(body: &RankingPreviewRequestDto) -> Result<WeightsDraft, ValidationError> {
    let weight = |raw: Option<i64>| raw.map(Weight::parse).transpose();
    let parameters = body
        .parameters
        .iter()
        .map(|draft| {
            Ok(ParameterWeightDraft {
                id: ParameterId::from_uuid(draft.id),
                weight: Weight::parse(draft.weight)?,
            })
        })
        .collect::<Result<_, ValidationError>>()?;
    Ok(WeightsDraft {
        heap: HeapPolicyPatch {
            need: weight(body.need)?,
            age: weight(body.age)?,
            deadline: weight(body.deadline)?,
            value: weight(body.value)?,
        },
        parameters,
    })
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
    responses((status = 201, body = ItemDto), (status = 404, body = Problem), (status = 409, description = "`heap_archived`", body = Problem), (status = 422, description = "Invalid fields, `value_unknown` (a `kind` the server doesn't know), `tag_not_in_heap`, `parameter_not_in_heap` or `option_not_in_parameter`", body = Problem)))]
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
        (status = 403, description = "From inside a run: `run_forbidden` (a field or state a run may not set), `run_wrong_item` (another item than the run's, named in `item_key`) or `run_done_needs_review`", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `item_closed`, `reopen_window_expired`, `heap_archived`, `kind_change_not_allowed`, `stage_needs_task`, or from inside a run `run_over`", body = Problem),
        (status = 422, description = "Invalid fields, `value_unknown` (a `kind` or `stage` the server doesn't know), `tag_not_in_heap`, `parameter_not_in_heap`, `option_not_in_parameter`, `item_not_blocked`, `blocker_not_in_heap`, `blocker_closed` or `blocking_cycle`", body = Problem),
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

/// Moves an item to another heap of its team, with its context, steps, questions, turn and history. Send the item's
/// `version` in `If-Match`. The item keeps its key unless the new heap has an item with it, and then gets a new one.
/// Tags and parameter values carry over where the new heap has a tag of the same name, or a parameter and option of
/// the same names; the rest are dropped. It keeps its state, and a blocked item stays blocked with nobody picked, as
/// do the items that waited for it. Moving an item to the heap it is in succeeds and changes nothing. The response
/// is the item in its new heap, so its old path stops working.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/move", tag = "items", security(("session" = [])),
    params(ItemParams, IfMatchHeader, TimeZoneHeader), request_body = ItemMoveDto,
    responses(
        (status = 200, body = ItemDto),
        (status = 403, description = "From inside a run: `run_forbidden`", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`version_conflict`, `item_closed` or `heap_archived` (either heap)", body = Problem),
        (status = 400, description = "A body that isn't a `heap_id`", body = Problem),
        (status = 422, description = "`heap_not_in_team`: the heap isn't another heap of the item's team, or doesn't exist", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn move_item(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<ItemMoveDto>,
) -> Result<Json<ItemDto>, ApiError> {
    let view = state
        .services
        .move_item(path.principal, path.item_id, HeapId::from_uuid(body.heap_id), expected)
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
