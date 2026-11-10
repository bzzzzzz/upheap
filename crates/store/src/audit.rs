//! The audit log. Insert-only: there is no update or delete function, on purpose.

use domain::audit::{AuditAction, EntityType};
use domain::{HeapId, TeamId, Timestamp, UserId};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::audit_log;

const TABLE: &str = "audit_log";

/// One audit entry to record. Insert it in the same transaction as the change it describes.
#[derive(Clone, Debug)]
pub struct NewAuditEntry {
    pub id: Uuid,
    pub team_id: TeamId,
    /// `None` for team-level actions.
    pub heap_id: Option<HeapId>,
    pub actor: UserId,
    pub action: AuditAction,
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    /// Changed fields as `{"field": {"before": …, "after": …}}`, or the created values. Never contains secrets.
    pub diff: serde_json::Value,
    pub created_at: Timestamp,
}

pub async fn insert_audit_entry(db: &impl ConnectionTrait, entry: &NewAuditEntry) -> Result<(), StoreError> {
    audit_log::Entity::insert(to_row(entry))
        .exec_without_returning(db)
        .await?;
    Ok(())
}

fn to_row(entry: &NewAuditEntry) -> audit_log::ActiveModel {
    audit_log::ActiveModel {
        id: Set(id_to_db(&entry.id)),
        team_id: Set(id_to_db(entry.team_id.as_uuid())),
        heap_id: Set(entry.heap_id.map(|h| id_to_db(h.as_uuid()))),
        actor_user_id: Set(id_to_db(entry.actor.as_uuid())),
        action: Set(entry.action.as_str().to_owned()),
        entity_type: Set(entry.entity_type.as_str().to_owned()),
        entity_id: Set(id_to_db(&entry.entity_id)),
        diff: Set(entry.diff.to_string()),
        created_at: Set(entry.created_at.as_millis()),
    }
}

/// Records `entries`, as [`insert_audit_entry`] does each, in a few statements.
pub async fn insert_audit_entries(db: &impl ConnectionTrait, entries: &[NewAuditEntry]) -> Result<(), StoreError> {
    // Well under the bind-parameter limits of SQLite and MySQL at 9 columns a row.
    const CHUNK: usize = 500;
    for chunk in entries.chunks(CHUNK) {
        audit_log::Entity::insert_many(chunk.iter().map(to_row))
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

/// A recorded audit entry, as read back.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditEntry {
    pub id: Uuid,
    pub team_id: TeamId,
    /// `None` for team-level actions.
    pub heap_id: Option<HeapId>,
    pub actor: UserId,
    pub action: AuditAction,
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    /// As written: see [`NewAuditEntry::diff`].
    pub diff: serde_json::Value,
    pub created_at: Timestamp,
}

/// Which part of a team's log to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditScope {
    /// Everything recorded about one heap and what is in it.
    Heap(HeapId),
    /// What is recorded about the team itself, its members, and its heaps as wholes (created, renamed, archived, context changed),
    /// but not what happens inside the heaps, which [`AuditScope::Heap`] reads.
    Team(TeamId),
}

/// Position after which a page of audit entries starts: the last entry of the previous page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditCursor {
    pub created_at: Timestamp,
    pub id: Uuid,
}

/// What to read from the log: newest first, `limit` entries of `actions` in `scope` after `after`, about
/// `entity_id` only when it is given.
#[derive(Clone, Copy, Debug)]
pub struct AuditQuery<'a> {
    pub scope: AuditScope,
    pub entity_id: Option<Uuid>,
    /// Only entries of these actions. Empty reads nothing.
    pub actions: &'a [AuditAction],
    pub after: Option<AuditCursor>,
    pub limit: u64,
}

/// The entries `query` asks for, newest first; entries made in the same millisecond come in reverse order of their
/// IDs, which is the reverse order they were made in.
pub async fn list_audit_entries(
    db: &impl ConnectionTrait,
    query: AuditQuery<'_>,
) -> Result<Vec<AuditEntry>, StoreError> {
    if query.actions.is_empty() {
        return Ok(Vec::new());
    }
    let mut select = audit_log::Entity::find()
        .filter(audit_log::Column::Action.is_in(query.actions.iter().map(|action| action.as_str())));
    select = match query.scope {
        AuditScope::Heap(heap_id) => select.filter(audit_log::Column::HeapId.eq(id_to_db(heap_id.as_uuid()))),
        AuditScope::Team(team_id) => select
            .filter(audit_log::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
            .filter(
                Condition::any()
                    .add(audit_log::Column::HeapId.is_null())
                    .add(audit_log::Column::EntityType.eq(EntityType::Heap.as_str())),
            ),
    };
    if let Some(entity_id) = query.entity_id {
        select = select.filter(audit_log::Column::EntityId.eq(id_to_db(&entity_id)));
    }
    if let Some(cursor) = query.after {
        let created_at = cursor.created_at.as_millis();
        select = select.filter(
            Condition::any().add(audit_log::Column::CreatedAt.lt(created_at)).add(
                Condition::all()
                    .add(audit_log::Column::CreatedAt.eq(created_at))
                    .add(audit_log::Column::Id.lt(id_to_db(&cursor.id))),
            ),
        );
    }
    select
        .order_by_desc(audit_log::Column::CreatedAt)
        .order_by_desc(audit_log::Column::Id)
        .limit(query.limit)
        .all(db)
        .await?
        .into_iter()
        .map(to_entry)
        .collect()
}

fn to_entry(row: audit_log::Model) -> Result<AuditEntry, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let action =
        AuditAction::from_code(&row.action).ok_or_else(|| read.corrupt(format!("unknown action {:?}", row.action)))?;
    let entity_type = EntityType::from_code(&row.entity_type)
        .ok_or_else(|| read.corrupt(format!("unknown entity type {:?}", row.entity_type)))?;
    let diff = serde_json::from_str(&row.diff).map_err(|e| read.corrupt(format!("diff is not JSON: {e}")))?;
    Ok(AuditEntry {
        id: read.uuid(&row.id)?,
        team_id: TeamId::from_uuid(read.uuid(&row.team_id)?),
        heap_id: row
            .heap_id
            .as_deref()
            .map(|h| read.uuid(h).map(HeapId::from_uuid))
            .transpose()?,
        actor: UserId::from_uuid(read.uuid(&row.actor_user_id)?),
        action,
        entity_type,
        entity_id: read.uuid(&row.entity_id)?,
        diff,
        created_at: read.timestamp(row.created_at)?,
    })
}
