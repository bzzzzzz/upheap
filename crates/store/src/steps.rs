//! A task's steps. Each is its own row with its own version, so writing one never touches the item's version.
//! Steps are never deleted: a dropped step keeps its row.

use std::collections::HashMap;

use domain::item::Title;
use domain::step::{Step, StepContext, StepNumber, StepState};
use domain::{ItemId, StepId, Version};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
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
    let rows = item_steps::Entity::find()
        .filter(item_steps::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .order_by_asc(item_steps::Column::Position)
        .order_by_asc(item_steps::Column::Number)
        .all(db)
        .await?;
    rows.into_iter().map(to_step).collect()
}

/// The steps of each of `item_ids` in position order, in one query per [`IN_LIST_CHUNK`] items. Items with no
/// steps are left out. Size assumption: a task has at most [`domain::step::MAX_STEPS`] steps, so a page of items
/// reads at most that many rows each.
pub async fn list_steps_for_items(
    db: &impl ConnectionTrait,
    item_ids: &[ItemId],
) -> Result<HashMap<ItemId, Vec<Step>>, StoreError> {
    let ids: Vec<String> = item_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut steps: HashMap<ItemId, Vec<Step>> = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_steps::Entity::find()
            .filter(item_steps::Column::ItemId.is_in(chunk.iter().map(String::as_str)))
            .order_by_asc(item_steps::Column::Position)
            .order_by_asc(item_steps::Column::Number)
            .all(db)
            .await?;
        for row in rows {
            let step = to_step(row)?;
            steps.entry(step.item_id).or_default().push(step);
        }
    }
    Ok(steps)
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
        Some(raw) => StepContext::parse(raw).map_err(|e| read.corrupt(e.to_string()))?,
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
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
