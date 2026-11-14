//! A task's steps. Each is its own row with its own version, so writing one never touches the item's version.
//! Steps are never deleted: a dropped step keeps its row.

use std::collections::HashMap;

use domain::item::{ItemSummary, Title};
use domain::step::{Step, StepNumber, StepProgress, StepState};
use domain::text::Context;
use domain::{ItemId, StepId, Version};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::{Expr, ExprTrait},
};

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db, version_to_db};
use crate::entities::item_steps;

const TABLE: &str = "item_steps";

/// Inserts a new step. The caller has made room for its position with [`shift_positions_after`].
pub async fn insert_step(db: &impl ConnectionTrait, step: &Step) -> Result<(), StoreError> {
    item_steps::Entity::insert(to_row(step)?)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// Moves every step of `item_id` placed after `position` one place down, to make room right after `position`.
pub async fn shift_positions_after(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    position: u32,
) -> Result<(), StoreError> {
    item_steps::Entity::update_many()
        .col_expr(
            item_steps::Column::Position,
            Expr::col(item_steps::Column::Position).add(1),
        )
        .filter(item_steps::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .filter(item_steps::Column::Position.gt(position_to_db(position, item_id)?))
        .exec(db)
        .await?;
    Ok(())
}

/// The steps of `item_id` in position order.
pub async fn list_steps(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Vec<Step>, StoreError> {
    read_steps(db, item_id, false).await
}

/// The steps of `item_id` in position order, read with a locking read (`FOR SHARE` on MySQL and Postgres), which
/// sees the latest committed rows rather than the transaction's snapshot. A step write calls it after
/// [`crate::heaps::lock_heap`], so on MySQL (REPEATABLE READ) it doesn't work from the snapshot fixed by an earlier
/// plain read: two racing adds then can't pick the same number, and the cap of
/// [`domain::step::MAX_STEPS`] holds. SQLite has no row locks and needs none (see `lock_heap`).
pub async fn list_steps_locking(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Vec<Step>, StoreError> {
    read_steps(db, item_id, true).await
}

/// [`list_steps`], or with `lock` [`list_steps_locking`].
async fn read_steps(db: &impl ConnectionTrait, item_id: ItemId, lock: bool) -> Result<Vec<Step>, StoreError> {
    let query = item_steps::Entity::find()
        .filter(item_steps::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .order_by_asc(item_steps::Column::Position)
        .order_by_asc(item_steps::Column::Number);
    let rows = crate::items::shared_if(query, lock).all(db).await?;
    rows.into_iter().map(to_step).collect()
}

/// The steps made from any of `origins` (their `origin_id` is one of them) with the task each is a step of, in
/// whichever heap that task is now, oldest first. With `lock` the steps are read with locking reads, as
/// [`list_steps_locking`] reads them.
///
/// Size assumption: a note becomes a step once, and the callers pass one note or one page of closed items (at most
/// 200), so the result is small.
pub async fn list_steps_made_from(
    db: &impl ConnectionTrait,
    origins: &[ItemId],
    lock: bool,
) -> Result<Vec<(Step, ItemSummary)>, StoreError> {
    let ids: Vec<String> = origins.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut steps = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let query =
            item_steps::Entity::find().filter(item_steps::Column::OriginId.is_in(chunk.iter().map(String::as_str)));
        for row in crate::items::shared_if(query, lock).all(db).await? {
            steps.push(to_step(row)?);
        }
    }
    steps.sort_by_key(|step| (step.created_at, *step.id.as_uuid()));
    let tasks: HashMap<ItemId, ItemSummary> = crate::items::list_blockers(db, steps.iter().map(|step| step.item_id))
        .await?
        .into_iter()
        .map(|task| (task.id, task))
        .collect();
    steps
        .into_iter()
        .map(|step| {
            let task = tasks
                .get(&step.item_id)
                .cloned()
                .ok_or_else(|| StoreError::corrupt(TABLE, &id_to_db(step.id.as_uuid()), "its item is missing"))?;
            Ok((step, task))
        })
        .collect()
}

/// The title of each step of `item_ids`, by item and step number, in one query per [`IN_LIST_CHUNK`] items. Only the
/// titles are read, since a step's context can be long.
pub async fn list_step_titles(
    db: &impl ConnectionTrait,
    item_ids: &[ItemId],
) -> Result<HashMap<(ItemId, StepNumber), Title>, StoreError> {
    let ids: Vec<String> = item_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut titles = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_steps::Entity::find()
            .select_only()
            .columns([
                item_steps::Column::Id,
                item_steps::Column::ItemId,
                item_steps::Column::Number,
                item_steps::Column::Title,
            ])
            .filter(item_steps::Column::ItemId.is_in(chunk.iter().map(String::as_str)))
            .into_tuple::<(String, String, i32, String)>()
            .all(db)
            .await?;
        for (id, item_id, number, title) in rows {
            let read = RowReader { table: TABLE, id: &id };
            let number = u32::try_from(number)
                .ok()
                .and_then(StepNumber::new)
                .ok_or_else(|| read.corrupt(format!("number {number} is not positive")))?;
            let title = Title::parse(&title).map_err(|e| read.corrupt(e.to_string()))?;
            titles.insert((ItemId::from_uuid(read.uuid(&item_id)?), number), title);
        }
    }
    Ok(titles)
}

/// The progress of each of `item_ids`, counted in one grouped query per [`IN_LIST_CHUNK`] items, without loading
/// the steps. Items with no open or ticked step are left out.
pub async fn count_step_progress(
    db: &impl ConnectionTrait,
    item_ids: &[ItemId],
) -> Result<HashMap<ItemId, StepProgress>, StoreError> {
    let ids: Vec<String> = item_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut counts: HashMap<ItemId, (usize, usize)> = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_steps::Entity::find()
            .select_only()
            .column_as(item_steps::Column::ItemId, "group_key")
            .column_as(item_steps::Column::State, "state")
            .column_as(item_steps::Column::Id.count(), "count")
            .filter(item_steps::Column::ItemId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_steps::Column::State.ne(StepState::Dropped.code()))
            .group_by(item_steps::Column::ItemId)
            .group_by(item_steps::Column::State)
            .into_model::<StateCount>()
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: TABLE,
                id: &row.group_key,
            };
            let state = StepState::from_code(&row.state)
                .ok_or_else(|| read.corrupt(format!("unknown state {:?}", row.state)))?;
            let count = usize::try_from(row.count).unwrap_or(usize::MAX);
            let entry = counts.entry(ItemId::from_uuid(read.uuid(&row.group_key)?)).or_default();
            entry.1 = entry.1.saturating_add(count);
            if state == StepState::Ticked {
                entry.0 = entry.0.saturating_add(count);
            }
        }
    }
    Ok(counts
        .into_iter()
        .filter_map(|(id, (ticked, total))| StepProgress::from_counts(ticked, total).map(|progress| (id, progress)))
        .collect())
}

/// One group of [`count_step_progress`]'s query: an item, a step state and how many of its steps are in it.
struct StateCount {
    group_key: String,
    state: String,
    count: i64,
}

impl sea_orm::FromQueryResult for StateCount {
    fn from_query_result(row: &sea_orm::QueryResult, pre: &str) -> Result<Self, sea_orm::DbErr> {
        let count = row.try_get::<i64>(pre, "count")?;
        Ok(Self {
            group_key: row.try_get(pre, "group_key")?,
            state: row.try_get(pre, "state")?,
            count,
        })
    }
}

/// Writes `step` if the stored row still has version `expected`, and returns it with its new version.
///
/// Returns `None` when the stored version differs, meaning someone else wrote first; nothing is written then. The
/// item, number and creation time never change, so they aren't written.
pub async fn update_step(
    db: &impl ConnectionTrait,
    step: &Step,
    expected: Version,
) -> Result<Option<Step>, StoreError> {
    let saved = Step {
        version: expected.next(),
        ..step.clone()
    };
    let id = id_to_db(step.id.as_uuid());
    let result = item_steps::Entity::update_many()
        .set(item_steps::ActiveModel {
            position: Set(position_to_db(saved.position, saved.item_id)?),
            title: Set(saved.title.as_str().to_owned()),
            context: Set(saved.context.as_ref().map(|context| context.as_str().to_owned())),
            state: Set(saved.state.code().to_owned()),
            version: Set(version_to_db(saved.version, TABLE, &id)?),
            ..Default::default()
        })
        .filter(item_steps::Column::Id.eq(id.as_str()))
        .filter(item_steps::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
}

fn position_to_db(position: u32, item_id: ItemId) -> Result<i32, StoreError> {
    i32::try_from(position).map_err(|_| StoreError::corrupt(TABLE, &id_to_db(item_id.as_uuid()), "position overflow"))
}

fn to_row(step: &Step) -> Result<item_steps::ActiveModel, StoreError> {
    let id = id_to_db(step.id.as_uuid());
    let number = i32::try_from(step.number.get()).map_err(|_| StoreError::corrupt(TABLE, &id, "number overflow"))?;
    Ok(item_steps::ActiveModel {
        version: Set(version_to_db(step.version, TABLE, &id)?),
        id: Set(id),
        item_id: Set(id_to_db(step.item_id.as_uuid())),
        number: Set(number),
        position: Set(position_to_db(step.position, step.item_id)?),
        title: Set(step.title.as_str().to_owned()),
        context: Set(step.context.as_ref().map(|context| context.as_str().to_owned())),
        state: Set(step.state.code().to_owned()),
        origin_id: Set(step.origin_id.map(|origin| id_to_db(origin.as_uuid()))),
        created_at: Set(step.created_at.as_millis()),
    })
}

fn to_step(row: item_steps::Model) -> Result<Step, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let number = u32::try_from(row.number)
        .ok()
        .and_then(StepNumber::new)
        .ok_or_else(|| read.corrupt(format!("number {} is not positive", row.number)))?;
    let position =
        u32::try_from(row.position).map_err(|_| read.corrupt(format!("position {} is negative", row.position)))?;
    let state =
        StepState::from_code(&row.state).ok_or_else(|| read.corrupt(format!("unknown state {:?}", row.state)))?;
    let context = match row.context.as_deref() {
        Some(raw) => Context::parse(raw).map_err(|e| read.corrupt(e.to_string()))?,
        None => None,
    };
    Ok(Step {
        id: StepId::from_uuid(read.uuid(&row.id)?),
        item_id: ItemId::from_uuid(read.uuid(&row.item_id)?),
        number,
        position,
        title: Title::parse(&row.title).map_err(|e| read.corrupt(e.to_string()))?,
        context,
        state,
        origin_id: row
            .origin_id
            .as_deref()
            .map(|raw| read.uuid(raw).map(ItemId::from_uuid))
            .transpose()?,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
