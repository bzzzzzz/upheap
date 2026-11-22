//! Items and the tags on them. Every query is scoped by heap where the caller knows the heap.
//!
//! An item's tags are rows in `item_tags`. Removing a tag sets `removed_at` rather than deleting the row, and
//! adding it again clears it. Its custom parameter values are rows in `item_parameter_values`; clearing one sets
//! `option_id` to `NULL`. Every function that returns an item returns it with its current tags and values, except
//! the summaries.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use domain::handle::ItemKey;
use domain::item::{
    BlockReason, Description, Due, Item, ItemState, ItemSummary, Kind, Need, Placement, Size, Stage, StateKind, Title,
};
use domain::{HeapId, ItemId, OptionId, ParameterId, RunId, TagId, Timestamp, UserId, Version};
use sea_orm::sea_query::{BinOper, Condition, Expr, ExprTrait, Func, Order};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Select,
};

use crate::StoreError;
use crate::codes::{GroupCount, IN_LIST_CHUNK, RowReader, id_to_db, version_to_db};
use crate::entities::{item_former_handles, item_parameter_values, item_tags, items};

const TABLE: &str = "items";

/// The character that escapes `%` and `_` in a search pattern, and itself.
const LIKE_ESCAPE: char = '!';

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

/// Records that the item `item_id` had `key` in `heap_id` before a move at `moved_at`.
pub async fn insert_former_handle(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    heap_id: HeapId,
    key: ItemKey,
    moved_at: Timestamp,
) -> Result<(), StoreError> {
    item_former_handles::Entity::insert(item_former_handles::ActiveModel {
        id: Set(id_to_db(&uuid::Uuid::now_v7())),
        item_id: Set(id_to_db(item_id.as_uuid())),
        heap_id: Set(id_to_db(heap_id.as_uuid())),
        key: Set(key.as_str().to_owned()),
        moved_at: Set(moved_at.as_millis()),
    })
    .exec_without_returning(db)
    .await?;
    Ok(())
}

/// The ID of the item that held `key` in `heap_id` before a move, or `None` when none did. Of several, the one that
/// left last.
pub async fn find_item_id_by_former_key(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    key: ItemKey,
) -> Result<Option<ItemId>, StoreError> {
    let row = item_former_handles::Entity::find()
        .select_only()
        .column(item_former_handles::Column::ItemId)
        .filter(item_former_handles::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(item_former_handles::Column::Key.eq(key.as_str()))
        .order_by_desc(item_former_handles::Column::MovedAt)
        .order_by_desc(item_former_handles::Column::Id)
        .into_tuple::<String>()
        .one(db)
        .await?;
    row.map(|id| {
        RowReader {
            table: "item_former_handles",
            id: &id,
        }
        .uuid(&id)
        .map(ItemId::from_uuid)
    })
    .transpose()
}

/// Whether an item of `heap_id` already has `key`. Run under the heap lock, so a new item can take the key safely.
pub async fn is_key_taken(db: &impl ConnectionTrait, heap_id: HeapId, key: ItemKey) -> Result<bool, StoreError> {
    Ok(find_item_id_by_key(db, heap_id, key).await?.is_some())
}

pub async fn find_item(db: &impl ConnectionTrait, id: ItemId) -> Result<Option<Item>, StoreError> {
    read_item(db, id, false).await
}

/// The item `id` with its tags and values, like [`find_item`], with the item row read with a locking read (`FOR
/// SHARE` on MySQL and Postgres), which sees the latest committed row rather than the transaction's snapshot. A write
/// that depends on the item's state (closed or open) calls it after [`crate::heaps::lock_heap`], so on MySQL
/// (REPEATABLE READ) it sees what committed while it waited for the lock. SQLite has no row locks and needs none (see
/// `lock_heap`).
///
/// The tags and values are plain reads, from the same snapshot as the reads that name them in a response, so none of
/// their IDs is ever missing there. One set while the write waited may be left out of its answer, as for any item
/// write, and so may a blocker or closer the locked row names that the snapshot doesn't have yet (it shows as
/// `null`); the next read has them.
pub async fn find_item_locking(db: &impl ConnectionTrait, id: ItemId) -> Result<Option<Item>, StoreError> {
    read_item(db, id, true).await
}

/// [`find_item`], or with `lock` [`find_item_locking`], which locks the item row only.
async fn read_item(db: &impl ConnectionTrait, id: ItemId, lock: bool) -> Result<Option<Item>, StoreError> {
    let row = shared_if(items::Entity::find_by_id(id_to_db(id.as_uuid())), lock)
        .one(db)
        .await?;
    let Some(item) = row.map(to_item).transpose()? else {
        return Ok(None);
    };
    let mut items = [item];
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    let [item] = items;
    Ok(Some(item))
}

/// `query`, as a locking read (`FOR SHARE` on MySQL and Postgres) when `lock` is set.
pub(crate) fn shared_if<E: EntityTrait>(query: Select<E>, lock: bool) -> Select<E> {
    if lock { query.lock_shared() } else { query }
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

/// The items `ids`, in any heap and state, without their tags and values, in one query per [`IN_LIST_CHUNK`] IDs, in no
/// particular order. Unknown IDs are left out.
pub async fn list_items_by_ids(db: &impl ConnectionTrait, ids: &[ItemId]) -> Result<Vec<Item>, StoreError> {
    let ids: Vec<String> = ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut found = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .filter(items::Column::Id.is_in(chunk.iter().map(String::as_str)))
            .all(db)
            .await?;
        for row in rows {
            found.push(to_item(row)?);
        }
    }
    Ok(found)
}

/// The summaries of the blockers `ids`, in whichever heap each is now, in no particular order.
///
/// Not scoped by heap, unlike the rest: a blocker moved to another heap stays the blocker of the items it left
/// behind, and a move stays in its team, so the caller, who may see the blocked item, may see its blocker.
pub async fn list_blockers(
    db: &impl ConnectionTrait,
    ids: impl IntoIterator<Item = ItemId>,
) -> Result<Vec<ItemSummary>, StoreError> {
    let ids: Vec<String> = ids.into_iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut summaries = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .filter(items::Column::Id.is_in(chunk.iter().map(String::as_str)))
            .all(db)
            .await?;
        for row in rows {
            summaries.push(ItemSummary::from(&to_item(row)?));
        }
    }
    Ok(summaries)
}

/// Every blocked item of `heap_ids` with a blocker picked, oldest first, without its tags and values. With
/// [`list_blockers`] it finds the items whose blocker is gone ([`Item::blocker_gone`]). A heap blocks tens of items,
/// so reading them all for Waiting for me and the agents' queue is cheap; a team with thousands would want the
/// blocker's state in this query instead.
pub async fn list_blocked_items_with_blocker(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
) -> Result<Vec<Item>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut items = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(items::Column::State.eq(StateKind::Blocked.code()))
            .filter(items::Column::BlockedByItemId.is_not_null())
            .order_by_asc(items::Column::CreatedAt)
            .order_by_asc(items::Column::Id)
            .all(db)
            .await?;
        for row in rows {
            items.push(to_item(row)?);
        }
    }
    Ok(items)
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

/// The items of `heap_id` that wait for `blocker`, with locking reads (`FOR SHARE` on MySQL and Postgres) like
/// [`find_item_locking`], oldest first. A write that closes `blocker` calls it after [`crate::heaps::lock_heap`].
pub async fn list_items_blocked_by_locking(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    blocker: ItemId,
) -> Result<Vec<Item>, StoreError> {
    let mut items = items::Entity::find()
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(items::Column::BlockedByItemId.eq(id_to_db(blocker.as_uuid())))
        .order_by_asc(items::Column::CreatedAt)
        .order_by_asc(items::Column::Id)
        .lock_shared()
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect::<Result<Vec<_>, _>>()?;
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    Ok(items)
}

/// The items split from any of `origins` (their `origin_id` is one of them and `made_in_split` is set), in any state
/// and any heap, oldest first,
/// with their tags and values. With `lock` the rows are read with locking reads (`FOR SHARE` on MySQL and Postgres)
/// like [`find_item_locking`], so a write that depends on what was made calls it after [`crate::heaps::lock_heap`].
///
/// Size assumption: a split makes a handful of items from a note, and the callers pass one note or one page of
/// closed items (at most 200), so the result is small. A note split into thousands would want paging here.
pub async fn list_items_made_from(
    db: &impl ConnectionTrait,
    origins: &[ItemId],
    lock: bool,
) -> Result<Vec<Item>, StoreError> {
    let ids: Vec<String> = origins.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut items = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let query = items::Entity::find()
            .filter(items::Column::OriginId.is_in(chunk.iter().map(String::as_str)))
            .filter(items::Column::MadeInSplit.eq(true))
            .order_by_asc(items::Column::CreatedAt)
            .order_by_asc(items::Column::Id);
        for row in shared_if(query, lock).all(db).await? {
            items.push(to_item(row)?);
        }
    }
    items.sort_by_key(|item| (item.created_at, *item.id.as_uuid()));
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    Ok(items)
}

/// Every item of `heap_id` in one of `states`, oldest first.
pub async fn list_items_in_states(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    states: &[StateKind],
) -> Result<Vec<Item>, StoreError> {
    read_items_in_states(db, heap_id, states, false).await
}

/// [`list_items_in_states`] with the item rows read with locking reads (`FOR SHARE` on MySQL and Postgres) like
/// [`find_item_locking`], so a write that works from the whole heap calls it after [`crate::heaps::lock_heap`] and
/// sees what committed while it waited, even on MySQL (REPEATABLE READ).
pub async fn list_items_in_states_locking(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    states: &[StateKind],
) -> Result<Vec<Item>, StoreError> {
    read_items_in_states(db, heap_id, states, true).await
}

async fn read_items_in_states(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    states: &[StateKind],
    lock: bool,
) -> Result<Vec<Item>, StoreError> {
    let query = items::Entity::find()
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(items::Column::State.is_in(states.iter().map(|s| s.code())))
        .order_by_asc(items::Column::CreatedAt)
        .order_by_asc(items::Column::Id);
    let mut items = shared_if(query, lock)
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect::<Result<Vec<_>, _>>()?;
    attach_tags(db, &mut items).await?;
    attach_values(db, &mut items).await?;
    Ok(items)
}

/// How many items each of `heap_ids` has in one of `states`, in one query per [`IN_LIST_CHUNK`] heaps. Heaps with
/// none are left out. A reminder whose date is after `now` doesn't count (`Item::is_to_come`): it isn't in the list
/// yet.
pub async fn count_items_by_heap(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    states: &[StateKind],
    now: Timestamp,
) -> Result<HashMap<HeapId, u32>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut counts = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .select_only()
            .column_as(items::Column::HeapId, "group_key")
            .column_as(items::Column::Id.count(), "count")
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(items::Column::State.is_in(states.iter().map(|s| s.code())))
            .filter(
                Condition::any()
                    .add(items::Column::Kind.ne(Kind::Reminder.code()))
                    .add(items::Column::ArrivesAt.lte(now.as_millis())),
            )
            .group_by(items::Column::HeapId)
            .into_model::<GroupCount>()
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: TABLE,
                id: &row.group_key,
            };
            counts.insert(HeapId::from_uuid(read.uuid(&row.group_key)?), row.count);
        }
    }
    Ok(counts)
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

/// Up to `limit` items of `heap_ids`, in any state and without their tags and values, whose title contains `text`
/// ignoring case, or whose key is `key`. The item with `key` comes first; then open items, newest created first; then
/// closed ones, most recently closed first.
///
/// The match is `LOWER(title) LIKE LOWER(pattern)`, which scans the heaps' items (the `heap_id` indexes only narrow it
/// to them) and folds case as the database's `LOWER` does: Unicode on Postgres and MySQL, ASCII only on SQLite; on MySQL the column's collation also ignores accents. It
/// holds while a person sees thousands of items at most; after that it moves to per-database full-text indexes.
/// `heap_ids` go in one `IN` list, which holds while a person views far fewer heaps than a backend's bound-parameter
/// limit; chunk it when that stops being true.
pub async fn search_items(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    key: Option<ItemKey>,
    text: &str,
    limit: u64,
) -> Result<Vec<Item>, StoreError> {
    if heap_ids.is_empty() {
        return Ok(Vec::new());
    }
    // `pattern ESCAPE '!'` is the right-hand side of the `LIKE`, as sea-query's own `LikeExpr` builds it, but with the
    // pattern lowered by the database, so both sides are folded the same way.
    let pattern = Expr::expr(Func::lower(Expr::val(like_pattern(text))))
        .binary(BinOper::Escape, Expr::Constant(LIKE_ESCAPE.into()));
    let title_matches = Expr::expr(Func::lower(Expr::col(items::Column::Title))).binary(BinOper::Like, pattern);
    let mut matches = Condition::any().add(title_matches);
    if let Some(key) = key {
        matches = matches.add(items::Column::Key.eq(key.as_str()));
    }
    let mut query = items::Entity::find()
        .filter(items::Column::HeapId.is_in(heap_ids.iter().map(|id| id_to_db(id.as_uuid()))))
        .filter(matches);
    if let Some(key) = key {
        query = query.order_by_asc(Into::<Expr>::into(
            Expr::case(items::Column::Key.eq(key.as_str()), 0).finally(1),
        ));
    }
    query
        .order_by_asc(Into::<Expr>::into(
            Expr::case(Expr::col(items::Column::ClosedAt).is_null(), 0).finally(1),
        ))
        .order_by(items::Column::ClosedAt, Order::Desc)
        .order_by_desc(items::Column::CreatedAt)
        .order_by_desc(items::Column::Id)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect()
}

/// `text` as a `LIKE` pattern that contains it, with `!` as the escape character, so `%`, `_` and `!` in `text` match
/// only themselves.
fn like_pattern(text: &str) -> String {
    let mut pattern = String::with_capacity(text.len() + 2);
    pattern.push('%');
    for c in text.chars() {
        if matches!(c, LIKE_ESCAPE | '%' | '_') {
            pattern.push(LIKE_ESCAPE);
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
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

/// The active `(item, tag)` links of the tags `tag_ids`, whatever the item's state and heap, in no particular order.
/// Serves the reach of a tag merge: how many items carry each tag, and which items to rewrite.
pub async fn list_active_tag_links(
    db: &impl ConnectionTrait,
    tag_ids: &[TagId],
) -> Result<Vec<(ItemId, TagId)>, StoreError> {
    let ids: Vec<String> = tag_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut links = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_tags::Entity::find()
            .filter(item_tags::Column::TagId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_tags::Column::RemovedAt.is_null())
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: "item_tags",
                id: &row.item_id,
            };
            links.push((
                ItemId::from_uuid(read.uuid(&row.item_id)?),
                TagId::from_uuid(read.uuid(&row.tag_id)?),
            ));
        }
    }
    Ok(links)
}

/// The set values `(item, parameter, option)` of the parameters `parameter_ids`, whatever the item's state and heap, in
/// no particular order. Serves the reach of a parameter merge: how many items have a value for each, and which items to
/// rewrite.
pub async fn list_parameter_values(
    db: &impl ConnectionTrait,
    parameter_ids: &[ParameterId],
) -> Result<Vec<(ItemId, ParameterId, OptionId)>, StoreError> {
    let ids: Vec<String> = parameter_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut values = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_parameter_values::Entity::find()
            .filter(item_parameter_values::Column::ParameterId.is_in(chunk.iter().map(String::as_str)))
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
            values.push((
                ItemId::from_uuid(read.uuid(&row.item_id)?),
                ParameterId::from_uuid(read.uuid(&row.parameter_id)?),
                OptionId::from_uuid(read.uuid(option)?),
            ));
        }
    }
    Ok(values)
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
        kind: Set(item.kind.code().to_owned()),
        stage: Set(item.stage.code().to_owned()),
        need: Set(item.need.code().to_owned()),
        effort: Set(item.effort.map(|s| s.code().to_owned())),
        impact: Set(item.impact.map(|s| s.code().to_owned())),
        due_at: Set(item.due.map(|d| d.at.as_millis())),
        due_firm: Set(item.due.map(|d| d.firm)),
        arrives_at: Set(item.arrives_at.map(Timestamp::as_millis)),
        placement: Set(item.placement.map(Placement::get)),
        created_at: Set(item.created_at.as_millis()),
        created_by: Set(item.created_by.map(|id| id_to_db(id.as_uuid()))),
        closed_at: Set(item.state.closed_at().map(Timestamp::as_millis)),
        closed_by: Set(item.state.closed_by().map(|id| id_to_db(id.as_uuid()))),
        blocked_by_item_id: Set(item.blocked_by().map(|id| id_to_db(id.as_uuid()))),
        blocked_reason: Set(item.blocked_reason().map(|reason| reason.as_str().to_owned())),
        origin_id: Set(item.origin_id.map(|id| id_to_db(id.as_uuid()))),
        created_run_id: Set(item.created_run_id.map(|id| id_to_db(id.as_uuid()))),
        made_in_split: Set(item.made_in_split),
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
    let blocked_reason = row
        .blocked_reason
        .as_deref()
        .map(|raw| BlockReason::parse(raw).map_err(|e| read.corrupt(e.to_string())))
        .transpose()?;
    let user = |raw: Option<&str>| raw.map(|raw| read.uuid(raw).map(UserId::from_uuid)).transpose();
    let closed_by = user(row.closed_by.as_deref())?;
    let state = ItemState::from_parts(
        kind,
        read.optional_timestamp(row.closed_at)?,
        closed_by,
        blocked_by,
        blocked_reason,
    )
    .map_err(|e| read.corrupt(e.to_string()))?;
    let item_kind = Kind::from_code(&row.kind).ok_or_else(|| read.corrupt(format!("unknown kind {:?}", row.kind)))?;
    let stage = Stage::from_code(&row.stage).ok_or_else(|| read.corrupt(format!("unknown stage {:?}", row.stage)))?;
    if !item_kind.allows(stage) {
        return Err(read.corrupt(format!("a {} can't be at stage {}", item_kind.code(), stage.code())));
    }
    let need = Need::from_code(&row.need).ok_or_else(|| read.corrupt(format!("unknown need {:?}", row.need)))?;
    let due = match (row.due_at, row.due_firm) {
        (None, None) => None,
        (Some(at), Some(firm)) => Some(Due {
            at: read.timestamp(at)?,
            firm,
        }),
        _ => return Err(read.corrupt("due_at and due_firm must be set together".to_owned())),
    };
    let arrives_at = read.optional_timestamp(row.arrives_at)?;
    if item_kind == Kind::Reminder && (arrives_at.is_none() || due.is_none()) {
        return Err(read.corrupt("a reminder needs a due and an arrival".to_owned()));
    }
    let placement = row
        .placement
        .map(|raw| Placement::new(raw).ok_or_else(|| read.corrupt(format!("invalid placement {raw}"))))
        .transpose()?;
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
        kind: item_kind,
        stage,
        need,
        effort: size(row.effort)?,
        impact: size(row.impact)?,
        due,
        arrives_at,
        // Filled in by `attach_tags` and `attach_values`; the row alone doesn't hold them.
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        created_at: read.timestamp(row.created_at)?,
        created_by: user(row.created_by.as_deref())?,
        origin_id: row
            .origin_id
            .as_deref()
            .map(|raw| read.uuid(raw).map(ItemId::from_uuid))
            .transpose()?,
        created_run_id: row
            .created_run_id
            .as_deref()
            .map(|raw| read.uuid(raw).map(RunId::from_uuid))
            .transpose()?,
        made_in_split: row.made_in_split,
        placement,
        version: read.version(row.version)?,
    })
}
