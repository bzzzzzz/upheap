//! The signed-in user, their teams and heaps.

pub(crate) mod history;
pub(crate) mod password;
pub(crate) mod sign_ins;

use api_types::common::HeapSummaryDto;
use api_types::common::{ItemActionDto, ItemRefDto};
use api_types::items::{AcrossHeapsItemDto, AcrossHeapsListDto, AcrossHeapsQuery};
use api_types::me::{HeapOverviewDto, MeActionDto, MeDto, MePatchDto, TeamDto, UserDto};
use api_types::waiting::{
    WaitingBlockerDto, WaitingDto, WaitingItemDto, WaitingRowDto, WaitingRowKindDto, WaitingStuckDto,
};
use axum::Json;
use axum::extract::State;
use domain::user::DisplayName;
use domain::{Timestamp, ValidationError};
use jiff::tz::TimeZone;
use service::ServiceError;
use service::across_heaps::AcrossHeapsEntry;
use service::me::{HeapOverview, MeView, TeamView};
use service::waiting::{WaitingRow, WaitingSubject};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::{IfMatchHeader, to_blocker_gone_dto, to_item_heap_dto, to_question_dto, to_ranked_dto, to_turn_dto};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiQuery, ClientTimeZone, CurrentUser, IfMatch, TimeZoneHeader};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_me, update_me))
        .routes(routes!(get_waiting))
        .routes(routes!(get_items))
}

/// What `GET /me` answers for `me`.
fn to_me_dto(me: MeView) -> MeDto {
    let mut allowed_actions = vec![
        MeActionDto::SignOut,
        MeActionDto::CreateTeam,
        MeActionDto::ManageSignIns,
    ];
    if me.has_password {
        allowed_actions.push(MeActionDto::ChangePassword);
    }
    let team = |view: TeamView| TeamDto {
        id: *view.team.id.as_uuid(),
        slug: view.team.slug.to_string(),
        name: view.team.name.as_str().to_owned(),
        personal: view.team.is_personal(),
        role: view.role.into(),
        member_count: view.member_count,
        heaps: view.heaps.iter().map(to_heap_overview_dto).collect(),
    };
    MeDto {
        user: UserDto::from(&me.user),
        teams: me.teams.into_iter().map(team).collect(),
        allowed_actions,
    }
}

fn to_heap_overview_dto(overview: &HeapOverview) -> HeapOverviewDto {
    HeapOverviewDto {
        heap: HeapSummaryDto::from(&overview.heap),
        open_count: overview.open_count,
    }
}

/// The signed-in user with their teams and heaps.
#[utoipa::path(get, path = "/me", tag = "me", security(("session" = [])),
    responses((status = 200, body = MeDto), (status = 401, description = "`unauthenticated`", body = Problem)))]
async fn get_me(State(state): State<AppState>, CurrentUser(principal): CurrentUser) -> Result<Json<MeDto>, ApiError> {
    Ok(Json(to_me_dto(state.services.load_me(principal).await?)))
}

/// Changes the signed-in user's display name. Send the user's `version` in `If-Match`.
#[utoipa::path(patch, path = "/me", tag = "me", security(("session" = [])),
    params(IfMatchHeader), request_body = MePatchDto,
    responses(
        (status = 200, body = UserDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 409, description = "`version_conflict`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_me(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<MePatchDto>,
) -> Result<Json<UserDto>, ApiError> {
    let display_name = DisplayName::parse(&body.display_name).map_err(ServiceError::from)?;
    let user = state
        .services
        .update_display_name(principal, display_name, expected)
        .await?;
    Ok(Json(UserDto::from(&user)))
}

fn to_waiting_row_dto(row: WaitingRow, as_of: Timestamp, tz: &TimeZone) -> WaitingRowDto {
    let item = row.item;
    let (kind, question, stuck, blocker) = match row.subject {
        WaitingSubject::Question(question) => (
            WaitingRowKindDto::Question,
            Some(to_question_dto(*question, as_of, tz)),
            None,
            None,
        ),
        WaitingSubject::Stuck(stuck) => (
            WaitingRowKindDto::Stuck,
            None,
            Some(WaitingStuckDto {
                failed_runs: stuck.failed_runs,
                reason: stuck.reason,
                goal: stuck.goal.into(),
                allowed_actions: stuck.allowed_actions.into_iter().map(ItemActionDto::from).collect(),
            }),
            None,
        ),
        WaitingSubject::BlockerGone(gone) => (
            WaitingRowKindDto::BlockerGone,
            None,
            None,
            Some(WaitingBlockerDto {
                item: ItemRefDto::from(gone.blocker),
                gone: to_blocker_gone_dto(gone.gone),
                allowed_actions: gone.allowed_actions.into_iter().map(ItemActionDto::from).collect(),
            }),
        ),
    };
    WaitingRowDto {
        kind,
        item: WaitingItemDto {
            id: *item.id.as_uuid(),
            key: item.key.to_string(),
            title: item.title.as_str().to_owned(),
            kind: item.kind.into(),
            stage: Some(item.stage.into()),
            turn: row.turn.map(|turn| to_turn_dto(turn, as_of, tz)),
            with_agent: row.with_agent,
            version: item.version.get(),
        },
        heap: to_item_heap_dto(&row.heap, &row.team),
        heap_priority: row.priority.get(),
        question,
        stuck,
        blocker,
        step_title: row.step_title.map(|title| title.as_str().to_owned()),
    }
}

/// What waits for the signed-in user: the open questions and requests on open items, the items whose agent turn is
/// stuck, and the blocked items whose blocker was closed (as won't do, or as done before it could free them) or moved
/// to another heap, in the active heaps of their teams, one row each, most important first. Rows are ordered by the item's
/// ranking score in its own heap times your priority for that heap, highest first (blockers' inheritance included),
/// then by heap name, the item's position, a stuck or blocker row before the item's questions, and the question's
/// number.
/// Questions on reminders to come have no score and come after the rest, by the reminder's date, soonest first. At
/// most 500 rows come back; `count` is the true total. Read-only, for any member.
#[utoipa::path(get, path = "/me/waiting", tag = "me", security(("session" = [])),
    params(TimeZoneHeader),
    responses((status = 200, body = WaitingDto), (status = 401, description = "`unauthenticated`", body = Problem)))]
async fn get_waiting(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<WaitingDto>, ApiError> {
    let waiting = state.services.list_waiting(principal).await?;
    let as_of = state.services.now();
    Ok(Json(WaitingDto {
        count: waiting.count,
        rows: waiting
            .rows
            .into_iter()
            .map(|row| to_waiting_row_dto(row, as_of, &tz))
            .collect(),
    }))
}

/// The non-closed items of every active heap the signed-in user can view, in one ranked list, each with its heap.
/// Every heap is ranked and grouped as its own list (`GET /teams/{team}/heaps/{heap}/items`) ranks it, so each row
/// carries the same reasons and actions; then each group is merged across heaps, ordered by the item's ranking score
/// in its own heap times your priority for that heap, highest first (blockers' inheritance included), then by heap
/// name, then the item's position in its heap. The view takes no tag, parameter or sort. At most 500 rows come back,
/// pinned first; `count` is the true total. Read-only, for any member.
#[utoipa::path(get, path = "/me/items", tag = "me", security(("session" = [])),
    params(AcrossHeapsQuery, TimeZoneHeader),
    responses(
        (status = 200, body = AcrossHeapsListDto),
        (status = 400, description = "`invalid_query`", body = Problem),
        (status = 401, description = "`unauthenticated`", body = Problem),
    ))]
async fn get_items(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<AcrossHeapsQuery>,
) -> Result<Json<AcrossHeapsListDto>, ApiError> {
    let as_of = query
        .as_of
        .map(|ms| Timestamp::from_millis(ms).ok_or(ValidationError::TimestampOutOfRange))
        .transpose()
        .map_err(ServiceError::from)?;
    let view = query.to_view().map_err(ServiceError::from)?;
    let list = state.services.list_ranked_across_heaps(principal, as_of, &view).await?;
    let as_of = list.as_of;
    let group = |entries: Vec<AcrossHeapsEntry>| {
        entries
            .into_iter()
            .map(|row| AcrossHeapsItemDto {
                position: row.position,
                heap: to_item_heap_dto(&row.heap, &row.team),
                heap_priority: row.priority.get(),
                entry: to_ranked_dto(row.entry, as_of, &tz),
            })
            .collect()
    };
    Ok(Json(AcrossHeapsListDto {
        as_of: as_of.as_millis(),
        count: list.count,
        pinned: group(list.pinned),
        ranked: group(list.ranked),
        blocked: group(list.blocked),
        hidden_count: list.hidden_count,
        in_progress_count: list.in_progress_count,
    }))
}
