//! Items and the tags on them. Every query is scoped by heap where the caller knows the heap.
//!
//! An item's tags are rows in `item_tags`. Removing a tag sets `removed_at` rather than deleting the row, and
//! adding it again clears it. Its custom parameter values are rows in `item_parameter_values`; clearing one sets
//! `option_id` to `NULL`. Every function that returns an item returns it with its current tags and values, except
//! the summaries.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use domain::handle::ItemKey;
use domain::item::{Description, Due, Item, ItemState, ItemSummary, Need, Size, StateKind, Title};
use domain::{HeapId, ItemId, OptionId, ParameterId, TagId, Timestamp, UserId, Version};
use sea_orm::sea_query::Condition;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{item_parameter_values, item_tags, items};

const TABLE: &str = "items";

/// Most IDs in one `IN (…)` list, well under every backend's limit on bound parameters.
const IN_LIST_CHUNK: usize = 500;

/// Position after which a page of closed items starts: the last item of the previous page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedCursor {
    pub closed_at: Timestamp,
    pub id: ItemId,
}

/// Inserts a new item with its tags and values. The caller has checked they belong to the item's heap.
pub async fn insert_item(db: &impl ConnectionTrait, item: &Item) -> Result<(), StoreError> {
    items::Entity::insert(to_row(item)?).exec_without_returning(db).await?;
    add_tags(db, item.id, &item.tags, item.created_at).await?;
    sync_values(db, item.id, &item.parameters).await
}

/// The ID of the item of `heap_id` with `key`, or `None` when the heap has none.
pub async fn find_item_id_by_key(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    key: ItemKey,
) -> Result<Option<ItemId>, StoreError> {
    let row = items::Entity::find()
        .select_only()
        .column(items::Column::Id)
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(items::Column::Key.eq(key.as_str()))
        .into_tuple::<String>()
        .one(db)
        .await?;
    row.map(|id| RowReader { table: TABLE, id: &id }.uuid(&id).map(ItemId::from_uuid))
        .transpose()
}

/// Whether an item of `heap_id` already has `key`. Run under the heap lock, so a new item can take the key safely.
pub async fn is_key_taken(db: &impl ConnectionTrait, heap_id: HeapId, key: ItemKey) -> Result<bool, StoreError> {
    Ok(find_item_id_by_key(db, heap_id, key).await?.is_some())
}

pub async fn find_item(db: &impl ConnectionTrait, id: ItemId) -> Result<Option<Item>, StoreError> {
    let row = items::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    let Some(item) = row.map(to_item).transpose()? else {
        return Ok(None);
    };
    let mut items = [item];
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    let [item] = items;
    Ok(Some(item))
}

/// The items among `ids` that belong to `heap_id`, in no particular order. IDs from other heaps, or unknown ones,
/// are left out.
pub async fn list_item_summaries(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: impl IntoIterator<Item = ItemId>,
) -> Result<Vec<ItemSummary>, StoreError> {
    let ids: Vec<String> = ids.into_iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut summaries = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
            .filter(items::Column::Id.is_in(chunk.iter().map(String::as_str)))
            .all(db)
            .await?;
        for row in rows {
            summaries.push(ItemSummary::from(&to_item(row)?));
        }
    }
    Ok(summaries)
}

/// The item `id` of `heap_id`, without its tags, or `None` when the heap has no such item.
///
/// It's a locking read (`FOR SHARE` on MySQL and Postgres), which sees the latest committed row rather than the
/// transaction's snapshot, so a check that walks the blocker chain after [`crate::heaps::lock_heap`] sees writes
/// that committed while it waited for the lock. SQLite has no row locks and needs none (see `lock_heap`).
pub async fn find_item_summary_locking(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    id: ItemId,
) -> Result<Option<ItemSummary>, StoreError> {
    items::Entity::find_by_id(id_to_db(id.as_uuid()))
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .lock_shared()
        .one(db)
        .await?
        .map(|row| to_item(row).map(|item| ItemSummary::from(&item)))
        .transpose()
}

/// Every item of `heap_id` in one of `states`, oldest first.
pub async fn list_items_in_states(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    states: &[StateKind],
) -> Result<Vec<Item>, StoreError> {
    let mut items = items::Entity::find()
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(items::Column::State.is_in(states.iter().map(|s| s.code())))
        .order_by_asc(items::Column::CreatedAt)
        .order_by_asc(items::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect::<Result<Vec<_>, _>>()?;
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    Ok(items)
}

/// Up to `limit` closed items of `heap_id`, most recently closed first, starting after `after`.
pub async fn list_closed_items(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    after: Option<ClosedCursor>,
    limit: u64,
) -> Result<Vec<Item>, StoreError> {
    let mut query = items::Entity::find()
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(items::Column::ClosedAt.is_not_null());
    if let Some(cursor) = after {
        let closed_at = cursor.closed_at.as_millis();
        query = query.filter(
            Condition::any().add(items::Column::ClosedAt.lt(closed_at)).add(
                Condition::all()
                    .add(items::Column::ClosedAt.eq(closed_at))
                    .add(items::Column::Id.lt(id_to_db(cursor.id.as_uuid()))),
            ),
        );
    }
    let mut items = query
        .order_by_desc(items::Column::ClosedAt)
        .order_by_desc(items::Column::Id)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect::<Result<Vec<_>, _>>()?;
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    Ok(items)
}

/// Writes `item` with its tags and values if the stored row still has version `expected`, and returns it with its
/// new version. Tags it no longer has are marked removed at `now`, and values it no longer has are cleared.
///
/// Returns `None` when the stored version differs, meaning someone else wrote first; nothing is written then.
pub async fn update_item(
    db: &impl ConnectionTrait,
    item: &Item,
    expected: Version,
    now: Timestamp,
) -> Result<Option<Item>, StoreError> {
    let next = expected.next();
    let saved = Item {
        version: next,
        ..item.clone()
    };
    let mut row = to_row(&saved)?;
    row.id = sea_orm::ActiveValue::NotSet;
    let id = id_to_db(item.id.as_uuid());
    let result = items::Entity::update_many()
        .set(row)
        .filter(items::Column::Id.eq(id.as_str()))
        .filter(items::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    if result.rows_affected != 1 {
        return Ok(None);
    }
    sync_tags(db, item.id, &item.tags, now).await?;
    sync_values(db, item.id, &item.parameters).await?;
    Ok(Some(saved))
}

/// Makes the item's set values exactly `values`: writes the new and changed ones and clears the others.
async fn sync_values(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    values: &BTreeMap<ParameterId, OptionId>,
) -> Result<(), StoreError> {
    let item = id_to_db(item_id.as_uuid());
    let stored: HashMap<String, Option<String>> = item_parameter_values::Entity::find()
        .filter(item_parameter_values::Column::ItemId.eq(item.as_str()))
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.parameter_id, row.option_id))
        .collect();
    let wanted: HashMap<String, String> = values
        .iter()
        .map(|(p, o)| (id_to_db(p.as_uuid()), id_to_db(o.as_uuid())))
        .collect();
    for (parameter, current) in &stored {
        let target = wanted.get(parameter).cloned();
        if *current == target {
            continue;
        }
        item_parameter_values::Entity::update_many()
            .col_expr(item_parameter_values::Column::OptionId, target.into())
            .filter(item_parameter_values::Column::ItemId.eq(item.as_str()))
            .filter(item_parameter_values::Column::ParameterId.eq(parameter.as_str()))
            .exec(db)
            .await?;
    }
    for (parameter, option) in wanted.iter().filter(|(p, _)| !stored.contains_key(*p)) {
        let row = item_parameter_values::ActiveModel {
            item_id: Set(item.clone()),
            parameter_id: Set(parameter.clone()),
            option_id: Set(Some(option.clone())),
        };
        item_parameter_values::Entity::insert(row)
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

/// Fills in each item's set values.
async fn attach_values(db: &impl ConnectionTrait, items: &mut [Item]) -> Result<(), StoreError> {
    let index: HashMap<String, usize> = items
        .iter()
        .enumerate()
        .map(|(i, item)| (id_to_db(item.id.as_uuid()), i))
        .collect();
    let keys: Vec<&String> = index.keys().collect();
    for chunk in keys.chunks(IN_LIST_CHUNK) {
        let rows = item_parameter_values::Entity::find()
            .filter(item_parameter_values::Column::ItemId.is_in(chunk.iter().map(|k| k.as_str())))
            .filter(item_parameter_values::Column::OptionId.is_not_null())
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: "item_parameter_values",
                id: &row.item_id,
            };
            let Some(option) = row.option_id.as_deref() else {
                continue;
            };
            let parameter = ParameterId::from_uuid(read.uuid(&row.parameter_id)?);
            let option = OptionId::from_uuid(read.uuid(option)?);
            if let Some(&i) = index.get(&row.item_id) {
                items[i].parameters.insert(parameter, option);
            }
        }
    }
    Ok(())
}

/// Makes the item's active tags exactly `tags`: adds the missing ones and marks the others removed.
async fn sync_tags(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    tags: &BTreeSet<TagId>,
    now: Timestamp,
) -> Result<(), StoreError> {
    let mut current = [(item_id, BTreeSet::new())];
    load_active_tags(db, &mut current).await?;
    let [(_, current)] = current;
    let removed: Vec<String> = current.difference(tags).map(|id| id_to_db(id.as_uuid())).collect();
    if !removed.is_empty() {
        item_tags::Entity::update_many()
            .col_expr(item_tags::Column::RemovedAt, now.as_millis().into())
            .filter(item_tags::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
            .filter(item_tags::Column::TagId.is_in(removed))
            .exec(db)
            .await?;
    }
    let added: BTreeSet<TagId> = tags.difference(&current).copied().collect();
    add_tags(db, item_id, &added, now).await
}

/// Adds `tags` to the item at `now`. A tag removed earlier gets its row back; a new one gets a new row.
async fn add_tags(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    tags: &BTreeSet<TagId>,
    now: Timestamp,
) -> Result<(), StoreError> {
    let item = id_to_db(item_id.as_uuid());
    for tag_id in tags {
        let tag = id_to_db(tag_id.as_uuid());
        let restored = item_tags::Entity::update_many()
            .col_expr(item_tags::Column::AddedAt, now.as_millis().into())
            .col_expr(item_tags::Column::RemovedAt, Option::<i64>::None.into())
            .filter(item_tags::Column::ItemId.eq(item.as_str()))
            .filter(item_tags::Column::TagId.eq(tag.as_str()))
            .filter(item_tags::Column::RemovedAt.is_not_null())
            .exec(db)
            .await?;
        if restored.rows_affected == 0 {
            let row = item_tags::ActiveModel {
                item_id: Set(item.clone()),
                tag_id: Set(tag),
                added_at: Set(now.as_millis()),
                removed_at: Set(None),
            };
            item_tags::Entity::insert(row).exec_without_returning(db).await?;
        }
    }
    Ok(())
}

/// Fills in each item's current tags.
async fn attach_tags(db: &impl ConnectionTrait, items: &mut [Item]) -> Result<(), StoreError> {
    let mut sets: Vec<(ItemId, BTreeSet<TagId>)> = items.iter().map(|item| (item.id, BTreeSet::new())).collect();
    load_active_tags(db, &mut sets).await?;
    for (item, (_, tags)) in items.iter_mut().zip(sets) {
        item.tags = tags;
    }
    Ok(())
}

/// Fills each `(item, set)` pair with the item's tags that haven't been removed.
async fn load_active_tags(db: &impl ConnectionTrait, sets: &mut [(ItemId, BTreeSet<TagId>)]) -> Result<(), StoreError> {
    let index: HashMap<String, usize> = sets
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (id_to_db(id.as_uuid()), i))
        .collect();
    let keys: Vec<&String> = index.keys().collect();
    for chunk in keys.chunks(IN_LIST_CHUNK) {
        let rows = item_tags::Entity::find()
            .filter(item_tags::Column::ItemId.is_in(chunk.iter().map(|k| k.as_str())))
            .filter(item_tags::Column::RemovedAt.is_null())
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: "item_tags",
                id: &row.item_id,
            };
            let tag_id = TagId::from_uuid(read.uuid(&row.tag_id)?);
            if let Some(&i) = index.get(&row.item_id) {
                sets[i].1.insert(tag_id);
            }
        }
    }
    Ok(())
}

fn to_row(item: &Item) -> Result<items::ActiveModel, StoreError> {
    let id = id_to_db(item.id.as_uuid());
    Ok(items::ActiveModel {
        version: Set(version_to_db(item.version, TABLE, &id)?),
        id: Set(id),
        heap_id: Set(id_to_db(item.heap_id.as_uuid())),
        key: Set(item.key.as_str().to_owned()),
        title: Set(item.title.as_str().to_owned()),
        description: Set(item.description.as_ref().map(|d| d.as_str().to_owned())),
        state: Set(item.state.kind().code().to_owned()),
        need: Set(item.need.code().to_owned()),
        effort: Set(item.effort.map(|s| s.code().to_owned())),
        impact: Set(item.impact.map(|s| s.code().to_owned())),
        due_at: Set(item.due.map(|d| d.at.as_millis())),
        due_firm: Set(item.due.map(|d| d.firm)),
        created_at: Set(item.created_at.as_millis()),
        created_by: Set(item.created_by.map(|id| id_to_db(id.as_uuid()))),
        closed_at: Set(item.state.closed_at().map(Timestamp::as_millis)),
        blocked_by_item_id: Set(item.blocked_by().map(|id| id_to_db(id.as_uuid()))),
    })
}

fn to_item(row: items::Model) -> Result<Item, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let kind =
        StateKind::from_code(&row.state).ok_or_else(|| read.corrupt(format!("unknown state {:?}", row.state)))?;
    let blocked_by = row
        .blocked_by_item_id
        .as_deref()
        .map(|raw| read.uuid(raw).map(ItemId::from_uuid))
        .transpose()?;
    let state = ItemState::from_parts(kind, read.optional_timestamp(row.closed_at)?, blocked_by)
        .map_err(|e| read.corrupt(e.to_string()))?;
    let need = Need::from_code(&row.need).ok_or_else(|| read.corrupt(format!("unknown need {:?}", row.need)))?;
    let due = match (row.due_at, row.due_firm) {
        (None, None) => None,
        (Some(at), Some(firm)) => Some(Due {
            at: read.timestamp(at)?,
            firm,
        }),
        _ => return Err(read.corrupt("due_at and due_firm must be set together".to_owned())),
    };
    let size = |raw: Option<String>| {
        raw.map(|r| Size::from_code(&r).ok_or_else(|| read.corrupt(format!("unknown size {r:?}"))))
            .transpose()
    };
    let description = match row.description.as_deref() {
        Some(raw) => Description::parse(raw).map_err(|e| read.corrupt(e.to_string()))?,
        None => None,
    };
    Ok(Item {
        id: ItemId::from_uuid(read.uuid(&row.id)?),
        heap_id: HeapId::from_uuid(read.uuid(&row.heap_id)?),
        key: read.item_key(&row.key)?,
        title: Title::parse(&row.title).map_err(|e| read.corrupt(e.to_string()))?,
        description,
        state,
        need,
        effort: size(row.effort)?,
        impact: size(row.impact)?,
        due,
        // Filled in by `attach_tags` and `attach_values`; the row alone doesn't hold them.
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        created_at: read.timestamp(row.created_at)?,
        created_by: row
            .created_by
            .as_deref()
            .map(|raw| read.uuid(raw).map(UserId::from_uuid))
            .transpose()?,
        version: read.version(row.version)?,
    })
}
