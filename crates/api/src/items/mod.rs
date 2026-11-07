//! The ranked list, closed items, and adding and editing items.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::item::{Description, ItemPatch, NewItem, StateKind, Title};
use std::collections::BTreeMap;

use domain::list_view::ListView;
use domain::parameter::ParameterError;
use domain::tag::parse_tag_set;
use domain::{ItemId, OptionId, ParameterId, TagId, Timestamp, ValidationError};
use jiff::tz::TimeZone;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Deserializer, Serialize};
use service::ServiceError;
use service::items::ClosedCursor;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::{
    DueInputDto, HeapSummaryDto, IfMatchHeader, ItemDto, RankedItemDto, SizeDto, StateDto, double_option, non_null,
    to_item_dto, to_ranked_dto,
};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiQuery, ClientTimeZone, IfMatch, TimeZoneHeader};
use crate::list_view::{BlockedLayoutDto, ListViewInputDto, SortDto, StateModeDto};
use crate::paths::{HeapParams, HeapPath, ItemParams, ItemPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_ranked, create_item))
        .routes(routes!(list_closed))
        .routes(routes!(get_item, update_item))
        .routes(routes!(reopen_item))
}

/// The list view. Every parameter is optional; leaving them all out gives the default view: in-progress items
/// pinned, open items shown, blocked items hidden, by priority.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RankedQuery {
    /// The `as_of` from an earlier response, to keep the order stable. Omit to rank at the current time.
    pub as_of: Option<i64>,
    /// Where open items go. Defaults to `shown`.
    pub open: Option<StateModeDto>,
    /// Where in-progress items go. Defaults to `pinned`.
    pub in_progress: Option<StateModeDto>,
    /// Where blocked items go. Defaults to `hidden`.
    pub blocked: Option<StateModeDto>,
    /// Whether shown blocked items get their own group or are mixed in with the open items. Defaults to `grouped`.
    pub blocked_layout: Option<BlockedLayoutDto>,
    /// What each group is sorted by. Ties keep priority order. Defaults to `priority`.
    pub sort: Option<SortDto>,
    /// The parameter to sort by, a parameter of the same heap. Send it exactly when `sort` is `parameter`.
    pub sort_parameter: Option<Uuid>,
    /// Keep items with any of these tags, comma-separated. Tags of the same heap.
    #[serde(default, deserialize_with = "comma_separated")]
    #[param(value_type = Option<Vec<Uuid>>, style = Form, explode = false)]
    pub tag: Vec<Uuid>,
    /// Keep items with any of these efforts, comma-separated.
    #[serde(default, deserialize_with = "comma_separated")]
    #[param(value_type = Option<Vec<SizeDto>>, style = Form, explode = false)]
    pub effort: Vec<SizeDto>,
    /// Keep items with any of these impacts, comma-separated.
    #[serde(default, deserialize_with = "comma_separated")]
    #[param(value_type = Option<Vec<SizeDto>>, style = Form, explode = false)]
    pub impact: Vec<SizeDto>,
    /// Options of the same heap's parameters, comma-separated. For each parameter they belong to, keep items with
    /// any of its options listed. An archived option keeps the items that switched to its replacement.
    #[serde(default, deserialize_with = "comma_separated")]
    #[param(value_type = Option<Vec<Uuid>>, style = Form, explode = false)]
    pub param: Vec<Uuid>,
}

impl RankedQuery {
    fn to_view(&self) -> Result<ListView, ValidationError> {
        ListViewInputDto {
            open: self.open,
            in_progress: self.in_progress,
            blocked: self.blocked,
            blocked_layout: self.blocked_layout,
            sort: self.sort,
            sort_parameter: self.sort_parameter,
            tag: self.tag.clone(),
            effort: self.effort.clone(),
            impact: self.impact.clone(),
            param: self.param.clone(),
        }
        .to_view()
    }
}

/// A comma-separated query value, such as `tag=a,b`. An empty value is an empty list.
fn comma_separated<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let raw = String::deserialize(deserializer)?;
    raw.split(',')
        .filter(|part| !part.is_empty())
        .map(|part| T::deserialize(part.to_owned().into_deserializer()))
        .collect()
}

#[derive(Serialize, ToSchema)]
#[schema(as = RankedList)]
pub(crate) struct RankedListDto {
    pub heap: HeapSummaryDto,
    /// The time the ranking used (UTC milliseconds). Send it back as `as_of` to keep the order stable.
    pub as_of: i64,
    /// Items in pinned states (in progress by default), above the rest.
    pub pinned: Vec<RankedItemDto>,
    /// Items in shown states, including blocked items when they're mixed in.
    pub ranked: Vec<RankedItemDto>,
    /// Shown blocked items, when they're grouped. Empty by default, because blocked items are hidden.
    pub blocked: Vec<RankedItemDto>,
    /// How many items the filters keep but are left out because their state is hidden.
    pub hidden_count: u32,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ClosedQuery {
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1 to 200. Defaults to 50.
    pub limit: Option<u64>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = ClosedItems)]
pub(crate) struct ClosedItemsDto {
    pub heap: HeapSummaryDto,
    /// Most recently closed first.
    pub items: Vec<ItemDto>,
    /// Pass as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = CreateItemRequest)]
pub(crate) struct CreateItemDto {
    pub title: String,
    pub description: Option<String>,
    pub effort: Option<SizeDto>,
    pub impact: Option<SizeDto>,
    pub due: Option<DueInputDto>,
    /// Tags of the same heap, at most 50.
    pub tag_ids: Option<Vec<Uuid>>,
    /// An option for each parameter to set, by parameter ID: options in use of the same heap's parameters in use.
    pub parameter_values: Option<BTreeMap<Uuid, Uuid>>,
}

/// A partial edit. Leave a field out to keep it; send `null` to clear it.
#[derive(Deserialize, ToSchema)]
#[schema(as = ItemPatchRequest)]
pub(crate) struct ItemPatchDto {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<SizeDto>, nullable)]
    pub effort: Option<Option<SizeDto>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<SizeDto>, nullable)]
    pub impact: Option<Option<SizeDto>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<DueInputDto>, nullable)]
    pub due: Option<Option<DueInputDto>>,
    /// Replaces the item's tags. Tags of the same heap, at most 50. Send `[]` to clear them; `null` is refused.
    #[serde(default, deserialize_with = "non_null")]
    #[schema(value_type = Vec<Uuid>, required = false)]
    pub tag_ids: Option<Vec<Uuid>>,
    /// Values to change, by parameter ID: an option in use to set, or `null` to clear. Parameters left out keep
    /// their values.
    #[serde(default, deserialize_with = "non_null")]
    #[schema(value_type = BTreeMap<Uuid, Option<Uuid>>, required = false)]
    pub parameter_values: Option<BTreeMap<Uuid, Option<Uuid>>>,
    pub state: Option<StateDto>,
    /// The item this one waits for: a non-closed item of the same heap. Only a blocked item can have one, so send
    /// it with `"state": "blocked"` unless the item is blocked already. `null` clears it. Leaving the blocked state
    /// clears it too.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub blocked_by_item_id: Option<Option<Uuid>>,
}

impl ItemPatchDto {
    fn to_patch(&self, tz: &TimeZone) -> Result<ItemPatch, ValidationError> {
        Ok(ItemPatch {
            title: self.title.as_deref().map(Title::parse).transpose()?,
            description: match &self.description {
                None => None,
                Some(None) => Some(None),
                Some(Some(raw)) => Some(Description::parse(raw)?),
            },
            effort: self.effort.map(|e| e.map(Into::into)),
            impact: self.impact.map(|i| i.map(Into::into)),
            due: match &self.due {
                None => None,
                Some(None) => Some(None),
                Some(Some(due)) => Some(Some(due.to_due(tz)?)),
            },
            tags: self.tag_ids.as_deref().map(to_tag_set).transpose()?,
            parameters: self
                .parameter_values
                .iter()
                .flatten()
                .map(|(parameter, option)| (ParameterId::from_uuid(*parameter), option.map(OptionId::from_uuid)))
                .collect(),
            state: self.state.map(StateKind::from),
            blocked_by: self.blocked_by_item_id.map(|id| id.map(ItemId::from_uuid)),
        })
    }
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
    let fields = to_new_item(&body, &tz).map_err(ServiceError::from)?;
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

fn to_new_item(body: &CreateItemDto, tz: &TimeZone) -> Result<NewItem, ValidationError> {
    Ok(NewItem {
        title: Title::parse(&body.title)?,
        description: body
            .description
            .as_deref()
            .map(Description::parse)
            .transpose()?
            .flatten(),
        effort: body.effort.map(Into::into),
        impact: body.impact.map(Into::into),
        due: body.due.as_ref().map(|due| due.to_due(tz)).transpose()?,
        tags: to_tag_set(body.tag_ids.as_deref().unwrap_or_default())?,
        parameters: body
            .parameter_values
            .iter()
            .flatten()
            .map(|(parameter, option)| (ParameterId::from_uuid(*parameter), OptionId::from_uuid(*option)))
            .collect(),
    })
}

fn to_tag_set(ids: &[Uuid]) -> Result<std::collections::BTreeSet<TagId>, ValidationError> {
    parse_tag_set(ids.iter().copied().map(TagId::from_uuid))
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
