//! Reading the audit log: a heap's history and a team's history, or one entity's within them.

use axum::Json;
use axum::extract::State;
use domain::Timestamp;
use domain::audit::{AuditAction, EntityType};
use serde::{Deserialize, Serialize};
use service::audit_log::{AuditChange, AuditCursor, AuditEntryView, AuditPage, AuditRequest};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::dto::UserRefDto;
use crate::error::Problem;
use crate::extract::{ApiQuery, ClientTimeZone, TimeZoneHeader};
use crate::paths::{HeapParams, HeapPath, TeamParams, TeamPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_heap_audit))
        .routes(routes!(list_team_audit))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct AuditQueryDto {
    /// Only the history of this entity: an item, tag, saved view or parameter of the heap, or the heap itself; for
    /// a team, the team, one of its heaps, or a member.
    pub entity_id: Option<Uuid>,
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1 to 200. Defaults to 50.
    pub limit: Option<u64>,
}

impl AuditQueryDto {
    fn to_request(&self) -> Result<AuditRequest, ApiError> {
        Ok(AuditRequest {
            entity_id: self.entity_id,
            after: self.cursor.as_deref().map(parse_cursor).transpose()?,
            limit: self.limit.unwrap_or(50),
        })
    }
}

/// What was done. Each action is shown to those who may take it: members see item, tag and saved view changes,
/// admins see everything.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AuditAction)]
pub(crate) enum AuditActionDto {
    UserRegistered,
    UserUpdated,
    TeamCreated,
    TeamUpdated,
    MemberAdded,
    MemberRoleChanged,
    MemberRemoved,
    MemberLeft,
    HeapCreated,
    HeapUpdated,
    HeapArchived,
    HeapUnarchived,
    ItemCreated,
    ItemUpdated,
    TagCreated,
    TagUpdated,
    SavedViewCreated,
    SavedViewUpdated,
    SavedViewArchived,
    ParameterCreated,
    ParameterUpdated,
    ParameterArchived,
    OptionArchived,
}

impl From<AuditAction> for AuditActionDto {
    fn from(action: AuditAction) -> Self {
        match action {
            AuditAction::UserRegistered => Self::UserRegistered,
            AuditAction::UserUpdated => Self::UserUpdated,
            AuditAction::TeamCreated => Self::TeamCreated,
            AuditAction::TeamUpdated => Self::TeamUpdated,
            AuditAction::MemberAdded => Self::MemberAdded,
            AuditAction::MemberRoleChanged => Self::MemberRoleChanged,
            AuditAction::MemberRemoved => Self::MemberRemoved,
            AuditAction::MemberLeft => Self::MemberLeft,
            AuditAction::HeapCreated => Self::HeapCreated,
            AuditAction::HeapUpdated => Self::HeapUpdated,
            AuditAction::HeapArchived => Self::HeapArchived,
            AuditAction::HeapUnarchived => Self::HeapUnarchived,
            AuditAction::ItemCreated => Self::ItemCreated,
            AuditAction::ItemUpdated => Self::ItemUpdated,
            AuditAction::TagCreated => Self::TagCreated,
            AuditAction::TagUpdated => Self::TagUpdated,
            AuditAction::SavedViewCreated => Self::SavedViewCreated,
            AuditAction::SavedViewUpdated => Self::SavedViewUpdated,
            AuditAction::SavedViewArchived => Self::SavedViewArchived,
            AuditAction::ParameterCreated => Self::ParameterCreated,
            AuditAction::ParameterUpdated => Self::ParameterUpdated,
            AuditAction::ParameterArchived => Self::ParameterArchived,
            AuditAction::OptionArchived => Self::OptionArchived,
        }
    }
}

/// The kind of thing an entry is about. Members are `user`.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AuditEntityType)]
pub(crate) enum EntityTypeDto {
    User,
    Team,
    Heap,
    Item,
    Tag,
    SavedView,
    Parameter,
}

impl From<EntityType> for EntityTypeDto {
    fn from(entity: EntityType) -> Self {
        match entity {
            EntityType::User => Self::User,
            EntityType::Team => Self::Team,
            EntityType::Heap => Self::Heap,
            EntityType::Item => Self::Item,
            EntityType::Tag => Self::Tag,
            EntityType::SavedView => Self::SavedView,
            EntityType::Parameter => Self::Parameter,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = AuditEntity)]
pub(crate) struct AuditEntityDto {
    #[serde(rename = "type")]
    pub entity_type: EntityTypeDto,
    pub id: Uuid,
    /// What it is called now: an item's title, a tag's name, a member's display name.
    pub label: String,
}

/// One changed field, ready to show.
#[derive(Serialize, ToSchema)]
#[schema(as = AuditChange)]
pub(crate) struct AuditChangeDto {
    /// The field as the diff names it: `title`, `tags`, or `parameters.<id>` for one parameter's value.
    pub field: String,
    /// The field in words.
    pub label: String,
    /// `null` when it had no value, and for values with no short form, such as what a saved view shows.
    pub before: Option<String>,
    pub after: Option<String>,
}

impl From<AuditChange> for AuditChangeDto {
    fn from(change: AuditChange) -> Self {
        Self {
            field: change.field,
            label: change.label,
            before: change.before,
            after: change.after,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = AuditEntry)]
pub(crate) struct AuditEntryDto {
    pub id: Uuid,
    /// UTC milliseconds.
    pub created_at: i64,
    /// `created_at` in the request's timezone, such as "12 Mar 2027, 14:05".
    pub created_label: String,
    pub actor: UserRefDto,
    pub action: AuditActionDto,
    /// What was done, to show after the actor's name: "edited item", "joined the team".
    pub phrase: String,
    /// Whether the entity's label goes after `phrase`. `false` when the entry is about the actor themselves.
    pub names_entity: bool,
    pub entity: AuditEntityDto,
    /// The changed fields, in the order the entity's form shows them. Names are the current ones.
    pub changes: Vec<AuditChangeDto>,
    /// The change as recorded, with IDs rather than names: `{"field": {"before": …, "after": …}}` for edits, the
    /// values set for creations.
    #[schema(value_type = Object)]
    pub diff: serde_json::Value,
}

impl From<AuditEntryView> for AuditEntryDto {
    fn from(view: AuditEntryView) -> Self {
        Self {
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
            changes: view.changes.into_iter().map(AuditChangeDto::from).collect(),
            diff: view.diff,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = AuditLog)]
pub(crate) struct AuditLogDto {
    /// Newest first.
    pub entries: Vec<AuditEntryDto>,
    /// Pass as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

impl From<AuditPage> for AuditLogDto {
    fn from(page: AuditPage) -> Self {
        Self {
            entries: page.entries.into_iter().map(AuditEntryDto::from).collect(),
            next_cursor: page.next.map(|c| format!("{}.{}", c.created_at.as_millis(), c.id)),
        }
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
        .list_heap_audit(path.principal, path.heap.heap_id, query.to_request()?, &tz)
        .await?;
    Ok(Json(page.into()))
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
        .list_team_audit(path.principal, path.team_id, query.to_request()?, &tz)
        .await?;
    Ok(Json(page.into()))
}

fn parse_cursor(raw: &str) -> Result<AuditCursor, ApiError> {
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
