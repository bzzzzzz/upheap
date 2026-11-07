//! Heaps and their ranking policy.

use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy};
use domain::text::Name;
use domain::{HeapId, TeamId, Timestamp, Version};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::heaps;

const TABLE: &str = "heaps";

/// Inserts `heap`. Fails with [`StoreError::Duplicate`] when another heap of the team has its slug.
pub async fn insert_heap(db: &impl ConnectionTrait, heap: &Heap, created_at: Timestamp) -> Result<(), StoreError> {
    let id = id_to_db(heap.id.as_uuid());
    let row = heaps::ActiveModel {
        version: Set(version_to_db(heap.version, TABLE, &id)?),
        id: Set(id),
        team_id: Set(id_to_db(heap.team_id.as_uuid())),
        slug: Set(heap.slug.as_str().to_owned()),
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

/// The heap of `team_id` with `slug`, or `None` if the team has none.
pub async fn find_heap_by_slug(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    slug: &Handle,
) -> Result<Option<Heap>, StoreError> {
    let row = heaps::Entity::find()
        .filter(heaps::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .filter(heaps::Column::Slug.eq(slug.as_str()))
        .one(db)
        .await?;
    row.map(to_heap).transpose()
}

/// Reads the heap and locks its row until the transaction ends, so writes that must see the whole heap at once, such
/// as the blocking-cycle check, run one at a time per heap.
///
/// Every item and tag write takes this lock before touching any other row. Their audit entries reference the heap,
/// so a write that locked an item first and the heap second could deadlock with one that went the other way.
///
/// MySQL takes a row lock (`FOR UPDATE`). SQLite has no row locks and allows one writer at a time anyway: a second
/// transaction that read before the first committed fails on its write instead of overwriting it.
///
/// The heap returned is read under the lock, so whether it is archived can't change before the transaction ends.
pub async fn lock_heap(db: &impl ConnectionTrait, id: HeapId) -> Result<Option<Heap>, StoreError> {
    let row = heaps::Entity::find_by_id(id_to_db(id.as_uuid()))
        .lock_exclusive()
        .one(db)
        .await?;
    row.map(to_heap).transpose()
}

/// Writes the slug, name, weights and archive time if the stored row still has version `expected`, and returns the
/// heap with its new version. Returns `None` when the stored version differs, and fails with
/// [`StoreError::Duplicate`] when another heap of the team has the slug.
pub async fn update_heap(
    db: &impl ConnectionTrait,
    heap: &Heap,
    expected: Version,
) -> Result<Option<Heap>, StoreError> {
    let id = id_to_db(heap.id.as_uuid());
    let saved = Heap {
        version: expected.next(),
        ..heap.clone()
    };
    let row = heaps::ActiveModel {
        slug: Set(saved.slug.as_str().to_owned()),
        name: Set(saved.name.as_str().to_owned()),
        age_weight: Set(i32::from(saved.policy.age_weight)),
        deadline_weight: Set(i32::from(saved.policy.deadline_weight)),
        value_weight: Set(i32::from(saved.policy.value_weight)),
        archived_at: Set(saved.archived_at.map(Timestamp::as_millis)),
        version: Set(version_to_db(saved.version, TABLE, &id)?),
        ..Default::default()
    };
    let result = heaps::Entity::update_many()
        .set(row)
        .filter(heaps::Column::Id.eq(id.as_str()))
        .filter(heaps::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
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
        slug: read.handle(&row.slug)?,
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
