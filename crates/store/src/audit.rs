//! The audit log. Insert-only: there is no update or delete function, on purpose.

use domain::audit::{AuditAction, EntityType};
use domain::{HeapId, TeamId, Timestamp, UserId};
use sea_orm::{ActiveValue::Set, ConnectionTrait, EntityTrait};
use uuid::Uuid;

use crate::StoreError;
use crate::codes::id_to_db;
use crate::entities::audit_log;

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
