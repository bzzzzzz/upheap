//! An item's turns. Each is its own row, so sending an item or taking it back never touches the item's version. An
//! item has at most one open turn (`ended_at` is `NULL`), which the service keeps under the heap lock. Turns are
//! never deleted: a taken-back one keeps its row.

use domain::turn::{Ended, Goal, Reach, Redone, Turn, TurnEnd, TurnLine, TurnSource};
use domain::{HeapId, ItemId, TurnId, UserId};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect,
};
use std::collections::HashMap;

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db};
use crate::entities::{item_turns, items};

const TABLE: &str = "item_turns";

/// Inserts a new turn.
pub async fn insert_turn(db: &impl ConnectionTrait, turn: &Turn) -> Result<(), StoreError> {
    item_turns::Entity::insert(to_row(turn))
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// The turn `id`, open or ended.
pub async fn find_turn(db: &impl ConnectionTrait, id: TurnId) -> Result<Option<Turn>, StoreError> {
    let row = item_turns::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_turn).transpose()
}

/// The open turn of `item_id`, if it has one.
pub async fn find_open_turn(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Option<Turn>, StoreError> {
    read_open_turn(db, item_id, false).await
}

/// [`find_open_turn`] as a locking read (`FOR SHARE` on MySQL and Postgres), which sees the latest committed rows
/// rather than the transaction's snapshot. A turn write calls it after [`crate::heaps::lock_heap`], so on MySQL
/// (REPEATABLE READ) it doesn't work from the snapshot fixed by an earlier plain read: two racing sends then can't
/// both find no open turn. SQLite has no row locks and needs none (see `lock_heap`).
pub async fn find_open_turn_locking(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Option<Turn>, StoreError> {
    read_open_turn(db, item_id, true).await
}

async fn read_open_turn(db: &impl ConnectionTrait, item_id: ItemId, lock: bool) -> Result<Option<Turn>, StoreError> {
    let query = item_turns::Entity::find()
        .filter(item_turns::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .filter(item_turns::Column::EndedAt.is_null());
    let row = crate::items::shared_if(query, lock).one(db).await?;
    row.map(to_turn).transpose()
}

/// The latest turn of `item_id`, open or ended: the one opened last, ties broken by ID. A locking read (see
/// [`find_open_turn_locking`]) when `lock` is set.
pub async fn find_latest_turn(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    lock: bool,
) -> Result<Option<Turn>, StoreError> {
    let query = item_turns::Entity::find()
        .filter(item_turns::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .order_by_desc(item_turns::Column::OpenedAt)
        .order_by_desc(item_turns::Column::Id);
    let row = crate::items::shared_if(query, lock).one(db).await?;
    row.map(to_turn).transpose()
}

/// Writes that `turn` was redone, if it ended as reached and isn't redone in the store. Returns whether it was:
/// `false` means someone redid it first, and nothing is written.
pub async fn mark_turn_redone(db: &impl ConnectionTrait, turn: &Turn) -> Result<bool, StoreError> {
    let Some(redone) = turn.redone else {
        return Ok(false);
    };
    let result = item_turns::Entity::update_many()
        .set(item_turns::ActiveModel {
            redone_at: Set(Some(redone.at.as_millis())),
            redone_by: Set(redone.by.map(|user| id_to_db(user.as_uuid()))),
            ..Default::default()
        })
        .filter(item_turns::Column::Id.eq(id_to_db(turn.id.as_uuid())))
        .filter(item_turns::Column::EndKind.eq(TurnEnd::Reached.code()))
        .filter(item_turns::Column::RedoneAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// The open turn of each of `item_ids` that has one, in one query per [`IN_LIST_CHUNK`] items. Items with none are
/// left out.
pub async fn list_open_turns(
    db: &impl ConnectionTrait,
    item_ids: &[ItemId],
) -> Result<HashMap<ItemId, Turn>, StoreError> {
    let ids: Vec<String> = item_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut turns = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_turns::Entity::find()
            .filter(item_turns::Column::ItemId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_turns::Column::EndedAt.is_null())
            .all(db)
            .await?;
        for row in rows {
            let turn = to_turn(row)?;
            turns.insert(turn.item_id, turn);
        }
    }
    Ok(turns)
}

/// Every open turn of the items of `heap_id`, as locking reads (see [`find_open_turn_locking`]), ordered by when they
/// opened. For archiving, which ends them all.
pub async fn list_open_turns_in_heap_locking(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
) -> Result<Vec<Turn>, StoreError> {
    let query = item_turns::Entity::find()
        .join(
            JoinType::InnerJoin,
            item_turns::Entity::belongs_to(items::Entity)
                .from(item_turns::Column::ItemId)
                .to(items::Column::Id)
                .into(),
        )
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(item_turns::Column::EndedAt.is_null());
    let mut turns = crate::items::shared_if(query, true)
        .all(db)
        .await?
        .into_iter()
        .map(to_turn)
        .collect::<Result<Vec<_>, _>>()?;
    turns.sort_by_key(|turn| (turn.opened_at, *turn.id.as_uuid()));
    Ok(turns)
}

/// Every open turn of the items of `heap_ids`, each with the heap of its item, in one query per [`IN_LIST_CHUNK`]
/// heaps, as locking reads when `lock` is set (see [`find_open_turn_locking`]). The order is the database's.
pub async fn list_open_turns_in_heaps(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    lock: bool,
) -> Result<Vec<(HeapId, Turn)>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut found = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let query = item_turns::Entity::find()
            .join(
                JoinType::InnerJoin,
                item_turns::Entity::belongs_to(items::Entity)
                    .from(item_turns::Column::ItemId)
                    .to(items::Column::Id)
                    .into(),
            )
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_turns::Column::EndedAt.is_null())
            .column_as(items::Column::HeapId, "heap_id");
        let rows = crate::items::shared_if(query, lock)
            .into_model::<TurnInHeap>()
            .all(db)
            .await?;
        for TurnInHeap { turn, heap_id } in rows {
            let read = RowReader {
                table: TABLE,
                id: &turn.id,
            };
            let heap_id = HeapId::from_uuid(read.uuid(&heap_id)?);
            found.push((heap_id, to_turn(turn)?));
        }
    }
    Ok(found)
}

/// A turn row with the heap of its item.
#[derive(sea_orm::FromQueryResult)]
struct TurnInHeap {
    #[sea_orm(nested)]
    turn: item_turns::Model,
    heap_id: String,
}

/// Sets how many runs in a row the open turn `id` has failed. Returns whether the turn was open: `false` means it
/// ended first, and nothing is written.
pub async fn set_failed_runs(db: &impl ConnectionTrait, id: TurnId, failed_runs: u32) -> Result<bool, StoreError> {
    let stored = i32::try_from(failed_runs).unwrap_or(i32::MAX);
    let result = item_turns::Entity::update_many()
        .set(item_turns::ActiveModel {
            failed_runs: Set(stored),
            ..Default::default()
        })
        .filter(item_turns::Column::Id.eq(id_to_db(id.as_uuid())))
        .filter(item_turns::Column::EndedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Writes how `turn` ended, if it is still open in the store. Returns whether it was: `false` means someone ended
/// it first, and nothing is written. An ended turn never changes again.
pub async fn end_turn(db: &impl ConnectionTrait, turn: &Turn) -> Result<bool, StoreError> {
    let Some(ended) = turn.ended else {
        return Ok(false);
    };
    let result = item_turns::Entity::update_many()
        .set(item_turns::ActiveModel {
            ended_at: Set(Some(ended.at.as_millis())),
            ended_by: Set(ended.by.map(|user| id_to_db(user.as_uuid()))),
            end_kind: Set(Some(ended.end.code().to_owned())),
            ..Default::default()
        })
        .filter(item_turns::Column::Id.eq(id_to_db(turn.id.as_uuid())))
        .filter(item_turns::Column::EndedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

fn to_row(turn: &Turn) -> item_turns::ActiveModel {
    let ended = turn.ended;
    item_turns::ActiveModel {
        id: Set(id_to_db(turn.id.as_uuid())),
        item_id: Set(id_to_db(turn.item_id.as_uuid())),
        reach: Set(turn.reach.code().to_owned()),
        goal: Set(turn.goal.code().to_owned()),
        line: Set(turn.line.as_ref().map(|line| line.as_str().to_owned())),
        opened_by: Set(turn.opened_by.map(|user| id_to_db(user.as_uuid()))),
        source: Set(turn.source.code().to_owned()),
        opened_at: Set(turn.opened_at.as_millis()),
        ended_at: Set(ended.map(|ended| ended.at.as_millis())),
        ended_by: Set(ended.and_then(|ended| ended.by).map(|user| id_to_db(user.as_uuid()))),
        end_kind: Set(ended.map(|ended| ended.end.code().to_owned())),
        redone_at: Set(turn.redone.map(|redone| redone.at.as_millis())),
        redone_by: Set(turn
            .redone
            .and_then(|redone| redone.by)
            .map(|user| id_to_db(user.as_uuid()))),
        failed_runs: Set(i32::try_from(turn.failed_runs).unwrap_or(i32::MAX)),
    }
}

fn to_turn(row: item_turns::Model) -> Result<Turn, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let reach = Reach::from_code(&row.reach).ok_or_else(|| read.corrupt(format!("unknown reach {:?}", row.reach)))?;
    let goal = Goal::from_code(&row.goal).ok_or_else(|| read.corrupt(format!("unknown goal {:?}", row.goal)))?;
    let source =
        TurnSource::from_code(&row.source).ok_or_else(|| read.corrupt(format!("unknown source {:?}", row.source)))?;
    let line = match row.line.as_deref() {
        Some(raw) => TurnLine::parse(raw).map_err(|e| read.corrupt(e.to_string()))?,
        None => None,
    };
    let ended = match (row.ended_at, row.end_kind.as_deref()) {
        (None, None) => None,
        (Some(at), Some(kind)) => Some(Ended {
            end: TurnEnd::from_code(kind).ok_or_else(|| read.corrupt(format!("unknown end {kind:?}")))?,
            at: read.timestamp(at)?,
            by: row
                .ended_by
                .as_deref()
                .map(|user| read.uuid(user).map(UserId::from_uuid))
                .transpose()?,
        }),
        _ => return Err(read.corrupt("ended_at and end_kind disagree".to_owned())),
    };
    let redone = match row.redone_at {
        None => None,
        Some(at) => Some(Redone {
            at: read.timestamp(at)?,
            by: row
                .redone_by
                .as_deref()
                .map(|user| read.uuid(user).map(UserId::from_uuid))
                .transpose()?,
        }),
    };
    Ok(Turn {
        id: TurnId::from_uuid(read.uuid(&row.id)?),
        item_id: ItemId::from_uuid(read.uuid(&row.item_id)?),
        reach,
        goal,
        line,
        opened_by: row
            .opened_by
            .as_deref()
            .map(|user| read.uuid(user).map(UserId::from_uuid))
            .transpose()?,
        source,
        opened_at: read.timestamp(row.opened_at)?,
        ended,
        redone,
        failed_runs: u32::try_from(row.failed_runs)
            .map_err(|_| read.corrupt(format!("failed_runs {} is negative", row.failed_runs)))?,
    })
}
