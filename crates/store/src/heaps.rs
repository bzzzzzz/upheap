//! Heaps and their ranking policy.

use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy};
use domain::parameter::Weight;
use domain::text::{Context, Name};
use domain::{HeapId, ItemId, TeamId, Timestamp, Version};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
};

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db, version_to_db};
use crate::entities::{heaps, items};

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
        need_weight: Set(weight_to_db(heap.policy.need_weight)),
        age_weight: Set(weight_to_db(heap.policy.age_weight)),
        deadline_weight: Set(weight_to_db(heap.policy.deadline_weight)),
        value_weight: Set(weight_to_db(heap.policy.value_weight)),
        context: Set(heap.context.as_ref().map(|c| c.as_str().to_owned())),
        agents_prepare: Set(heap.agents_prepare),
        origin_id: Set(heap.origin_id.map(|origin| id_to_db(origin.as_uuid()))),
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
/// MySQL and Postgres take a row lock (`FOR UPDATE`). SQLite has no row locks and allows one writer at a time
/// anyway: a second transaction that read before the first committed fails on its write instead of overwriting it.
///
/// The heap returned is read under the lock, so whether it is archived can't change before the transaction ends.
pub async fn lock_heap(db: &impl ConnectionTrait, id: HeapId) -> Result<Option<Heap>, StoreError> {
    let row = heaps::Entity::find_by_id(id_to_db(id.as_uuid()))
        .lock_exclusive()
        .one(db)
        .await?;
    row.map(to_heap).transpose()
}

/// Writes the slug, name, context, prepare setting, weights and archive time if the stored row still has version `expected`, and
/// returns the heap with its new version. Returns `None` when the stored version differs, and fails with
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
        need_weight: Set(weight_to_db(saved.policy.need_weight)),
        age_weight: Set(weight_to_db(saved.policy.age_weight)),
        deadline_weight: Set(weight_to_db(saved.policy.deadline_weight)),
        value_weight: Set(weight_to_db(saved.policy.value_weight)),
        context: Set(saved.context.as_ref().map(|c| c.as_str().to_owned())),
        agents_prepare: Set(saved.agents_prepare),
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

/// The heaps made from any of `origins` (their `origin_id` is one of them), oldest first.
pub async fn list_heaps_made_from(db: &impl ConnectionTrait, origins: &[ItemId]) -> Result<Vec<Heap>, StoreError> {
    let ids: Vec<String> = origins.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut found = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = heaps::Entity::find()
            .filter(heaps::Column::OriginId.is_in(chunk.iter().map(String::as_str)))
            .all(db)
            .await?;
        for row in rows {
            found.push((row.created_at, to_heap(row)?));
        }
    }
    found.sort_by_key(|(created_at, heap)| (*created_at, *heap.id.as_uuid()));
    Ok(found.into_iter().map(|(_, heap)| heap).collect())
}

/// The heaps made from an item that is in one of `heap_ids`, oldest first.
pub async fn list_heaps_made_from_items_in(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
) -> Result<Vec<Heap>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut found = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let in_heaps = items::Entity::find()
            .select_only()
            .column(items::Column::Id)
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)));
        let rows = heaps::Entity::find()
            .filter(heaps::Column::OriginId.in_subquery(in_heaps.into_query()))
            .all(db)
            .await?;
        for row in rows {
            found.push((row.created_at, to_heap(row)?));
        }
    }
    found.sort_by_key(|(created_at, heap)| (*created_at, *heap.id.as_uuid()));
    Ok(found.into_iter().map(|(_, heap)| heap).collect())
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
            need_weight: read.weight(row.need_weight)?,
            age_weight: read.weight(row.age_weight)?,
            deadline_weight: read.weight(row.deadline_weight)?,
            value_weight: read.weight(row.value_weight)?,
        },
        context: match row.context.as_deref() {
            Some(raw) => Context::parse(raw).map_err(|e| read.corrupt(e.to_string()))?,
            None => None,
        },
        agents_prepare: row.agents_prepare,
        origin_id: row
            .origin_id
            .as_deref()
            .map(|raw| read.uuid(raw).map(ItemId::from_uuid))
            .transpose()?,
        archived_at: read.optional_timestamp(row.archived_at)?,
        version: read.version(row.version)?,
    })
}

fn weight_to_db(weight: Weight) -> i32 {
    i32::from(weight.get())
}
