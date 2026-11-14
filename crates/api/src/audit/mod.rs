//! Reading the audit log: a heap's history and a team's history, or one entity's within them.

use api_types::audit::{AuditChangeDto, AuditEntityDto, AuditEntryDto, AuditLogDto, AuditQueryDto, AuditRunDto};
use api_types::common::UserRefDto;
use axum::Json;
use axum::extract::State;
use domain::Timestamp;
use service::audit_log::{AuditChange, AuditCursor, AuditEntryView, AuditPage, AuditRequest, AuditRun};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::to_item_origin_dto;
use crate::error::Problem;
use crate::extract::{ApiQuery, ClientTimeZone, TimeZoneHeader};
use crate::paths::{HeapParams, HeapPath, TeamParams, TeamPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_heap_audit))
        .routes(routes!(list_team_audit))
}

fn to_audit_request(query: &AuditQueryDto) -> Result<AuditRequest, ApiError> {
    Ok(AuditRequest {
        entity_id: query.entity_id,
        after: query.cursor.as_deref().map(parse_cursor).transpose()?,
        limit: query.limit.unwrap_or(50),
    })
}

fn to_audit_change_dto(change: AuditChange) -> AuditChangeDto {
    AuditChangeDto {
        field: change.field,
        label: change.label,
        before: change.before,
        after: change.after,
    }
}

fn to_audit_entry_dto(view: AuditEntryView) -> AuditEntryDto {
    AuditEntryDto {
        id: view.id,
        created_at: view.created_at.as_millis(),
        created_label: view.created_label,
        actor: UserRefDto {
            id: *view.actor.as_uuid(),
            display_name: view.actor_name,
        },
        action: view.action.into(),
        phrase: view.phrase.to_owned(),
        names_entity: view.names_entity,
        entity: AuditEntityDto {
            entity_type: view.entity_type.into(),
            id: view.entity_id,
            label: view.entity_label,
        },
        changes: view.changes.into_iter().map(to_audit_change_dto).collect(),
        diff: view.diff,
        run: view.run.map(to_audit_run_dto),
    }
}

fn to_audit_run_dto(run: AuditRun) -> AuditRunDto {
    AuditRunDto {
        id: *run.id.as_uuid(),
        item: run.item.map(to_item_origin_dto),
    }
}

fn to_audit_log_dto(page: AuditPage) -> AuditLogDto {
    AuditLogDto {
        entries: page.entries.into_iter().map(to_audit_entry_dto).collect(),
        next_cursor: page.next.map(format_cursor),
    }
}

/// What happened in a heap, newest first, as far as the caller may see: changes to items, tags and saved views
/// for every member, and to the heap and its parameters for admins too.
#[utoipa::path(get, path = "/teams/{team}/heaps/{heap}/audit", tag = "audit", security(("session" = [])),
    params(HeapParams, AuditQueryDto, TimeZoneHeader),
    responses((status = 200, body = AuditLogDto), (status = 400, description = "`invalid_cursor`", body = Problem), (status = 404, body = Problem)))]
async fn list_heap_audit(
    State(state): State<AppState>,
    path: HeapPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<AuditQueryDto>,
) -> Result<Json<AuditLogDto>, ApiError> {
    let page = state
        .services
        .list_heap_audit(path.principal, path.heap.heap_id, to_audit_request(&query)?, &tz)
        .await?;
    Ok(Json(to_audit_log_dto(page)))
}

/// What happened to a team, newest first, as far as the caller may see: its name, members and display names, and
/// its heaps being created, renamed and archived. Membership and heap changes are shown to admins only.
#[utoipa::path(get, path = "/teams/{team}/audit", tag = "audit", security(("session" = [])),
    params(TeamParams, AuditQueryDto, TimeZoneHeader),
    responses((status = 200, body = AuditLogDto), (status = 400, description = "`invalid_cursor`", body = Problem), (status = 404, body = Problem)))]
async fn list_team_audit(
    State(state): State<AppState>,
    path: TeamPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<AuditQueryDto>,
) -> Result<Json<AuditLogDto>, ApiError> {
    let page = state
        .services
        .list_team_audit(path.principal, path.team_id, to_audit_request(&query)?, &tz)
        .await?;
    Ok(Json(to_audit_log_dto(page)))
}

/// The opaque cursor a page hands out, which `parse_cursor` reads back.
pub(crate) fn format_cursor(cursor: AuditCursor) -> String {
    format!("{}.{}", cursor.created_at.as_millis(), cursor.id)
}

pub(crate) fn parse_cursor(raw: &str) -> Result<AuditCursor, ApiError> {
    let invalid = || ApiError::BadRequest {
        code: "invalid_cursor",
        detail: "cursor is not one this server issued".to_owned(),
    };
    let (millis, id) = raw.split_once('.').ok_or_else(invalid)?;
    let created_at = millis
        .parse()
        .ok()
        .and_then(Timestamp::from_millis)
        .ok_or_else(invalid)?;
    let id = Uuid::parse_str(id).map_err(|_| invalid())?;
    Ok(AuditCursor { created_at, id })
}
