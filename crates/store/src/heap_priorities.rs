//! A person's priority for each heap (`domain::heap::HeapPriority`). A heap the person never set one for has no row
//! and reads as the default. Rows outlive a team membership, so rejoining restores the priority, and are never
//! deleted.

use std::collections::HashMap;

use domain::heap::HeapPriority;
use domain::{HeapId, Timestamp, UserId};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::heap_priorities::{self, Column};

const TABLE: &str = "heap_priorities";

/// The priorities `user_id` has set for any of `heap_ids`. Heaps with no row are absent: callers use
/// [`HeapPriority::DEFAULT`] for them.
pub async fn list_heap_priorities(
    db: &impl ConnectionTrait,
    user_id: UserId,
    heap_ids: &[HeapId],
) -> Result<HashMap<HeapId, HeapPriority>, StoreError> {
    heap_priorities::Entity::find()
        .filter(Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(Column::HeapId.is_in(heap_ids.iter().map(|id| id_to_db(id.as_uuid()))))
        .all(db)
        .await?
        .into_iter()
        .map(|row| {
            let read = RowReader {
                table: TABLE,
                id: &row.heap_id,
            };
            let priority = HeapPriority::parse(i64::from(row.priority)).map_err(|e| read.corrupt(e.to_string()))?;
            Ok((HeapId::from_uuid(read.uuid(&row.heap_id)?), priority))
        })
        .collect()
}

/// Sets `user_id`'s priority for `heap_id`, replacing the one stored. Last write wins.
pub async fn upsert_heap_priority(
    db: &impl ConnectionTrait,
    user_id: UserId,
    heap_id: HeapId,
    priority: HeapPriority,
    now: Timestamp,
) -> Result<(), StoreError> {
    let row = heap_priorities::ActiveModel {
        user_id: Set(id_to_db(user_id.as_uuid())),
        heap_id: Set(id_to_db(heap_id.as_uuid())),
        priority: Set(i16::from(priority.get())),
        updated_at: Set(now.as_millis()),
    };
    let on_conflict = OnConflict::columns([Column::UserId, Column::HeapId])
        .update_columns([Column::Priority, Column::UpdatedAt])
        .to_owned();
    heap_priorities::Entity::insert(row)
        .on_conflict(on_conflict)
        .exec_without_returning(db)
        .await?;
    Ok(())
}
