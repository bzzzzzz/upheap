//! Heaps and their ranking policy.

use domain::heap::{Heap, HeapPolicy};
use domain::text::Name;
use domain::{HeapId, TeamId, Timestamp};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::heaps;

const TABLE: &str = "heaps";

pub async fn insert_heap(db: &impl ConnectionTrait, heap: &Heap, created_at: Timestamp) -> Result<(), StoreError> {
    let id = id_to_db(heap.id.as_uuid());
    let row = heaps::ActiveModel {
        version: Set(version_to_db(heap.version, TABLE, &id)?),
        id: Set(id),
        team_id: Set(id_to_db(heap.team_id.as_uuid())),
        name: Set(heap.name.as_str().to_owned()),
        age_weight: Set(i32::from(heap.policy.age_weight)),
        deadline_weight: Set(i32::from(heap.policy.deadline_weight)),
        value_weight: Set(i32::from(heap.policy.value_weight)),
        created_at: Set(created_at.as_millis()),
        archived_at: Set(heap.archived_at.map(Timestamp::as_millis)),
    };
    heaps::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

pub async fn find_heap(db: &impl ConnectionTrait, id: HeapId) -> Result<Option<Heap>, StoreError> {
    let row = heaps::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_heap).transpose()
}

/// Locks the heap's row until the transaction ends, so writes that must see the whole heap at once, such as the
/// blocking-cycle check, run one at a time per heap.
///
/// Every item and tag write takes this lock before touching any other row. Their audit entries reference the heap,
/// so a write that locked an item first and the heap second could deadlock with one that went the other way.
///
/// MySQL takes a row lock (`FOR UPDATE`). SQLite has no row locks and allows one writer at a time anyway: a second
/// transaction that read before the first committed fails on its write instead of overwriting it.
pub async fn lock_heap(db: &impl ConnectionTrait, id: HeapId) -> Result<(), StoreError> {
    heaps::Entity::find_by_id(id_to_db(id.as_uuid()))
        .lock_exclusive()
        .one(db)
        .await?;
    Ok(())
}

/// Every heap of the given teams, archived ones included, ordered by name.
pub async fn list_heaps_for_teams(db: &impl ConnectionTrait, team_ids: &[TeamId]) -> Result<Vec<Heap>, StoreError> {
    heaps::Entity::find()
        .filter(heaps::Column::TeamId.is_in(team_ids.iter().map(|id| id_to_db(id.as_uuid()))))
        .order_by_asc(heaps::Column::Name)
        .order_by_asc(heaps::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_heap)
        .collect()
}

fn to_heap(row: heaps::Model) -> Result<Heap, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    Ok(Heap {
        id: HeapId::from_uuid(read.uuid(&row.id)?),
        team_id: TeamId::from_uuid(read.uuid(&row.team_id)?),
        name: Name::parse(&row.name).map_err(|e| read.corrupt(e.to_string()))?,
        policy: HeapPolicy {
            age_weight: read.weight(row.age_weight)?,
            deadline_weight: read.weight(row.deadline_weight)?,
            value_weight: read.weight(row.value_weight)?,
        },
        archived_at: read.optional_timestamp(row.archived_at)?,
        version: read.version(row.version)?,
    })
}
