//! The reads behind the Data page: the runs started, the items created and the items closed in a period, over a set
//! of heaps. Each returns plain rows, one per run or item; the caller buckets them by day or week in the client's
//! time zone, which SQL can't do portably across SQLite, MySQL and Postgres.
//!
//! Size assumption: the rows returned are a team's items and runs in the longest period (91 days), thousands at most,
//! so they fit in memory. The created-items and runs reads filter by time after selecting by heap, so they scan every
//! item of the team's heaps and those items' runs: fine at the thousands of items a heap holds today. Beyond that, add
//! an index on `items (heap_id, created_at)` and on `item_runs (started_at)`, then count with `GROUP BY` on precomputed
//! UTC day columns.

use domain::data::{ClosedRow, RunRow};
use domain::item::StateKind;
use domain::run::RunEnd;
use domain::{HeapId, Timestamp};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, JoinType, QueryFilter, QuerySelect};

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db};
use crate::entities::{item_runs, items};

/// The runs that started in `[from, to)` on the items now in `heap_ids`, with how they ended and what they cost.
pub async fn list_runs_started(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    from: Timestamp,
    to: Timestamp,
) -> Result<Vec<RunRow>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut runs = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_runs::Entity::find()
            .join(
                JoinType::InnerJoin,
                item_runs::Entity::belongs_to(items::Entity)
                    .from(item_runs::Column::ItemId)
                    .to(items::Column::Id)
                    .into(),
            )
            .select_only()
            .column(item_runs::Column::Id)
            .column(item_runs::Column::StartedAt)
            .column(item_runs::Column::EndKind)
            .column(item_runs::Column::CostMicroUsd)
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_runs::Column::StartedAt.gte(from.as_millis()))
            .filter(item_runs::Column::StartedAt.lt(to.as_millis()))
            .into_tuple::<(String, i64, Option<String>, Option<i64>)>()
            .all(db)
            .await?;
        for (id, started_at, end_kind, cost_micro_usd) in rows {
            let read = RowReader {
                table: "item_runs",
                id: &id,
            };
            let end = end_kind
                .as_deref()
                .map(|code| RunEnd::from_code(code).ok_or_else(|| read.corrupt(format!("unknown end_kind {code:?}"))))
                .transpose()?;
            runs.push(RunRow {
                started_at: read.timestamp(started_at)?,
                end,
                cost_micro_usd,
            });
        }
    }
    Ok(runs)
}

/// When the items now in `heap_ids` were created, for those created in `[from, to)`.
pub async fn list_items_created(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    from: Timestamp,
    to: Timestamp,
) -> Result<Vec<Timestamp>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut created = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .select_only()
            .column(items::Column::Id)
            .column(items::Column::CreatedAt)
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(items::Column::CreatedAt.gte(from.as_millis()))
            .filter(items::Column::CreatedAt.lt(to.as_millis()))
            .into_tuple::<(String, i64)>()
            .all(db)
            .await?;
        for (id, created_at) in rows {
            let read = RowReader {
                table: "items",
                id: &id,
            };
            created.push(read.timestamp(created_at)?);
        }
    }
    Ok(created)
}

/// The items now in `heap_ids` that are closed, with when and how, for those closed in `[from, to)`. A reopened item
/// has no `closed_at`, so it isn't among them.
pub async fn list_items_closed(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    from: Timestamp,
    to: Timestamp,
) -> Result<Vec<ClosedRow>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut closed = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .select_only()
            .column(items::Column::Id)
            .column(items::Column::ClosedAt)
            .column(items::Column::State)
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(items::Column::ClosedAt.gte(from.as_millis()))
            .filter(items::Column::ClosedAt.lt(to.as_millis()))
            .into_tuple::<(String, i64, String)>()
            .all(db)
            .await?;
        for (id, closed_at, state) in rows {
            let read = RowReader {
                table: "items",
                id: &id,
            };
            let state = StateKind::from_code(&state)
                .filter(|state| state.is_closed())
                .ok_or_else(|| read.corrupt(format!("closed item has state {state:?}")))?;
            closed.push(ClosedRow {
                closed_at: read.timestamp(closed_at)?,
                state,
            });
        }
    }
    Ok(closed)
}
