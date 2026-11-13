//! An item's runs. Each is its own row, so claiming one, hearing from it and ending it never touch the item's
//! version. A run ends once (`ended_at` is set from `NULL`) and its cost is set once; neither changes again. Runs are
//! never deleted.
//!
//! A run that nobody has heard from for [`domain::run::CLAIM_LAPSE`] is still stored as not ended until something
//! touches it: whether it is lapsed is the caller's to say, from `heard_at` and the time (architecture.md, Runs).

use std::collections::HashMap;

use domain::item::Stage;
use domain::run::{Run, RunEnd, RunEnded, RunLink, RunResult};
use domain::turn::Goal;
use domain::{HeapId, ItemId, RunId, Timestamp, TurnId, UserId};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect,
};

use crate::StoreError;
use crate::codes::{GroupCount, IN_LIST_CHUNK, RowReader, id_to_db};
use crate::entities::{item_runs, item_turns, items};

const TABLE: &str = "item_runs";

/// Inserts a new run.
pub async fn insert_run(db: &impl ConnectionTrait, run: &Run) -> Result<(), StoreError> {
    item_runs::Entity::insert(to_row(run))
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// The run `id`, as a locking read (`FOR SHARE` on MySQL and Postgres), which sees the latest committed row rather
/// than the transaction's snapshot. A write in a run calls it after [`crate::heaps::lock_heap`], so on MySQL
/// (REPEATABLE READ) it doesn't work from the snapshot fixed by an earlier plain read. SQLite has no row locks and
/// needs none (see `lock_heap`).
pub async fn find_run_locking(db: &impl ConnectionTrait, id: RunId) -> Result<Option<Run>, StoreError> {
    let row = crate::items::shared_if(item_runs::Entity::find_by_id(id_to_db(id.as_uuid())), true)
        .one(db)
        .await?;
    row.map(to_run).transpose()
}

/// The run `id` as a plain read, to find which item it holds before the heap is locked.
pub async fn find_run(db: &impl ConnectionTrait, id: RunId) -> Result<Option<Run>, StoreError> {
    let row = item_runs::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_run).transpose()
}

/// The first run of the turn `turn_id`, the one that started earliest, as a locking read (see [`find_run_locking`]).
pub async fn find_earliest_run_for_turn_locking(
    db: &impl ConnectionTrait,
    turn_id: TurnId,
) -> Result<Option<Run>, StoreError> {
    let query = item_runs::Entity::find()
        .filter(item_runs::Column::TurnId.eq(id_to_db(turn_id.as_uuid())))
        .order_by_asc(item_runs::Column::StartedAt)
        .order_by_asc(item_runs::Column::Id);
    let row = crate::items::shared_if(query, true).one(db).await?;
    row.map(to_run).transpose()
}

/// The runs of the turn `turn_id` that have not ended, as locking reads (see [`find_run_locking`]). A turn has at most
/// one in practice: a claim skips an item with a live run, and writes a lapsed one before choosing.
pub async fn list_unended_runs_for_turn_locking(
    db: &impl ConnectionTrait,
    turn_id: TurnId,
) -> Result<Vec<Run>, StoreError> {
    let query = item_runs::Entity::find()
        .filter(item_runs::Column::TurnId.eq(id_to_db(turn_id.as_uuid())))
        .filter(item_runs::Column::EndedAt.is_null())
        .order_by_asc(item_runs::Column::StartedAt);
    crate::items::shared_if(query, true)
        .all(db)
        .await?
        .into_iter()
        .map(to_run)
        .collect()
}

/// The runs that have not ended among the turns `turn_ids`, in one query per [`IN_LIST_CHUNK`] turns, as locking reads
/// when `lock` is set. Live ones and lapsed ones both: the caller tells them apart by time.
pub async fn list_unended_runs_for_turns(
    db: &impl ConnectionTrait,
    turn_ids: &[TurnId],
    lock: bool,
) -> Result<Vec<Run>, StoreError> {
    let ids: Vec<String> = turn_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut runs = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let query = item_runs::Entity::find()
            .filter(item_runs::Column::TurnId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_runs::Column::EndedAt.is_null())
            .order_by_asc(item_runs::Column::StartedAt)
            .order_by_asc(item_runs::Column::Id);
        for row in crate::items::shared_if(query, lock).all(db).await? {
            runs.push(to_run(row)?);
        }
    }
    Ok(runs)
}

/// The runs that have not ended on the items of `heap_ids`, in one query per [`IN_LIST_CHUNK`] heaps, as locking reads
/// when `lock` is set. Live ones and lapsed ones both: the caller tells them apart by time.
pub async fn list_unended_runs_in_heaps(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    lock: bool,
) -> Result<Vec<Run>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut runs = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let query = item_runs::Entity::find()
            .join(
                JoinType::InnerJoin,
                item_runs::Entity::belongs_to(items::Entity)
                    .from(item_runs::Column::ItemId)
                    .to(items::Column::Id)
                    .into(),
            )
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_runs::Column::EndedAt.is_null())
            .order_by_asc(item_runs::Column::StartedAt)
            .order_by_asc(item_runs::Column::Id);
        for row in crate::items::shared_if(query, lock).all(db).await? {
            runs.push(to_run(row)?);
        }
    }
    Ok(runs)
}

/// Writes how `run` ended, if it has not ended in the store. Returns whether it had not: `false` means someone ended
/// it first, and nothing is written.
pub async fn end_run(db: &impl ConnectionTrait, run: &Run) -> Result<bool, StoreError> {
    let Some(ended) = &run.ended else {
        return Ok(false);
    };
    let links = (!ended.links.is_empty()).then(|| links_to_db(&ended.links));
    let result = item_runs::Entity::update_many()
        .set(item_runs::ActiveModel {
            ended_at: Set(Some(ended.at.as_millis())),
            end_kind: Set(Some(ended.end.code().to_owned())),
            result: Set(ended.result.as_ref().map(|result| result.as_str().to_owned())),
            links: Set(links),
            failure: Set(ended.failure.clone()),
            ..Default::default()
        })
        .filter(item_runs::Column::Id.eq(id_to_db(run.id.as_uuid())))
        .filter(item_runs::Column::EndedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Sets the cost of the run `id`, if it has none. Returns whether it set it: `false` means the cost was already set.
pub async fn set_run_cost(db: &impl ConnectionTrait, id: RunId, cost_micro_usd: i64) -> Result<bool, StoreError> {
    let result = item_runs::Entity::update_many()
        .set(item_runs::ActiveModel {
            cost_micro_usd: Set(Some(cost_micro_usd)),
            ..Default::default()
        })
        .filter(item_runs::Column::Id.eq(id_to_db(id.as_uuid())))
        .filter(item_runs::Column::CostMicroUsd.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Sets the tool start and exit times of the run `id`, each only where it is still unset: a run's times are never
/// changed once set, so a second call changes nothing.
pub async fn set_run_tool_times(
    db: &impl ConnectionTrait,
    id: RunId,
    started: Option<Timestamp>,
    ended: Option<Timestamp>,
) -> Result<(), StoreError> {
    if let Some(started) = started {
        item_runs::Entity::update_many()
            .set(item_runs::ActiveModel {
                tool_started_at: Set(Some(started.as_millis())),
                ..Default::default()
            })
            .filter(item_runs::Column::Id.eq(id_to_db(id.as_uuid())))
            .filter(item_runs::Column::ToolStartedAt.is_null())
            .exec(db)
            .await?;
    }
    if let Some(ended) = ended {
        item_runs::Entity::update_many()
            .set(item_runs::ActiveModel {
                tool_ended_at: Set(Some(ended.as_millis())),
                ..Default::default()
            })
            .filter(item_runs::Column::Id.eq(id_to_db(id.as_uuid())))
            .filter(item_runs::Column::ToolEndedAt.is_null())
            .exec(db)
            .await?;
    }
    Ok(())
}

/// The runs of the item `item_id`, newest first (by start, then id), each with the goal of its turn, as locking reads
/// when `lock` is set.
pub async fn list_runs_for_item(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    lock: bool,
) -> Result<Vec<(Run, Goal)>, StoreError> {
    let query = item_runs::Entity::find()
        .filter(item_runs::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .order_by_desc(item_runs::Column::StartedAt)
        .order_by_desc(item_runs::Column::Id);
    let rows = crate::items::shared_if(query, lock).all(db).await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    // The item's turns are few, and no relation joins them to runs: read their goals and match by id.
    let turns = crate::items::shared_if(
        item_turns::Entity::find().filter(item_turns::Column::ItemId.eq(id_to_db(item_id.as_uuid()))),
        lock,
    )
    .all(db)
    .await?;
    let goals: HashMap<&str, &str> = turns
        .iter()
        .map(|turn| (turn.id.as_str(), turn.goal.as_str()))
        .collect();
    rows.into_iter()
        .map(|row| {
            let read = RowReader {
                table: TABLE,
                id: &row.id,
            };
            let code = goals
                .get(row.turn_id.as_str())
                .ok_or_else(|| read.corrupt("run has no turn".to_owned()))?;
            let goal = Goal::from_code(code).ok_or_else(|| read.corrupt(format!("unknown goal {code:?}")))?;
            Ok((to_run(row)?, goal))
        })
        .collect()
}

/// Records that the run `id` was heard from at `heard_at`, if it has not ended and not been heard from later.
pub async fn touch_run(db: &impl ConnectionTrait, id: RunId, heard_at: domain::Timestamp) -> Result<(), StoreError> {
    item_runs::Entity::update_many()
        .set(item_runs::ActiveModel {
            heard_at: Set(heard_at.as_millis()),
            ..Default::default()
        })
        .filter(item_runs::Column::Id.eq(id_to_db(id.as_uuid())))
        .filter(item_runs::Column::EndedAt.is_null())
        .filter(item_runs::Column::HeardAt.lt(heard_at.as_millis()))
        .exec(db)
        .await?;
    Ok(())
}

/// How many runs each of `turn_ids` has had, ended or not, in one grouped query per [`IN_LIST_CHUNK`] turns. Turns
/// with none are left out.
pub async fn count_runs_for_turns(
    db: &impl ConnectionTrait,
    turn_ids: &[TurnId],
) -> Result<HashMap<TurnId, u32>, StoreError> {
    let ids: Vec<String> = turn_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut counts = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_runs::Entity::find()
            .select_only()
            .column_as(item_runs::Column::TurnId, "group_key")
            .column_as(item_runs::Column::Id.count(), "count")
            .filter(item_runs::Column::TurnId.is_in(chunk.iter().map(String::as_str)))
            .group_by(item_runs::Column::TurnId)
            .into_model::<GroupCount>()
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: TABLE,
                id: &row.group_key,
            };
            counts.insert(TurnId::from_uuid(read.uuid(&row.group_key)?), row.count);
        }
    }
    Ok(counts)
}

/// The failure text of the latest failed run of each of `turn_ids` that has one, in one query per [`IN_LIST_CHUNK`]
/// turns. Turns with no failed run, and failed runs with no text, are left out.
pub async fn latest_failure_for_turns(
    db: &impl ConnectionTrait,
    turn_ids: &[TurnId],
) -> Result<HashMap<TurnId, String>, StoreError> {
    let ids: Vec<String> = turn_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut latest = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_runs::Entity::find()
            .filter(item_runs::Column::TurnId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_runs::Column::EndKind.eq(RunEnd::Failed.code()))
            .order_by_asc(item_runs::Column::EndedAt)
            .order_by_asc(item_runs::Column::StartedAt)
            .order_by_asc(item_runs::Column::Id)
            .all(db)
            .await?;
        // Ascending, so the last run of a turn wins.
        for row in rows {
            let read = RowReader {
                table: TABLE,
                id: &row.id,
            };
            let turn = TurnId::from_uuid(read.uuid(&row.turn_id)?);
            match row.failure {
                Some(failure) => latest.insert(turn, failure),
                None => latest.remove(&turn),
            };
        }
    }
    Ok(latest)
}

fn links_to_db(links: &[RunLink]) -> String {
    let links: Vec<&str> = links.iter().map(RunLink::as_str).collect();
    serde_json::to_string(&links).unwrap_or_else(|_| "[]".to_owned())
}

fn to_row(run: &Run) -> item_runs::ActiveModel {
    let ended = run.ended.as_ref();
    item_runs::ActiveModel {
        id: Set(id_to_db(run.id.as_uuid())),
        item_id: Set(id_to_db(run.item_id.as_uuid())),
        turn_id: Set(id_to_db(run.turn_id.as_uuid())),
        user_id: Set(id_to_db(run.user_id.as_uuid())),
        started_at: Set(run.started_at.as_millis()),
        heard_at: Set(run.heard_at.as_millis()),
        ended_at: Set(ended.map(|ended| ended.at.as_millis())),
        end_kind: Set(ended.map(|ended| ended.end.code().to_owned())),
        from_stage: Set(run.from_stage.code().to_owned()),
        result: Set(ended.and_then(|ended| ended.result.as_ref().map(|result| result.as_str().to_owned()))),
        links: Set(ended.and_then(|ended| (!ended.links.is_empty()).then(|| links_to_db(&ended.links)))),
        failure: Set(ended.and_then(|ended| ended.failure.clone())),
        cost_micro_usd: Set(run.cost_micro_usd),
        tool_started_at: Set(run.tool_started_at.map(Timestamp::as_millis)),
        tool_ended_at: Set(run.tool_ended_at.map(Timestamp::as_millis)),
    }
}

fn to_run(row: item_runs::Model) -> Result<Run, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let from_stage =
        Stage::from_code(&row.from_stage).ok_or_else(|| read.corrupt(format!("unknown stage {:?}", row.from_stage)))?;
    let ended = match (row.ended_at, row.end_kind.as_deref()) {
        (None, None) => None,
        (Some(at), Some(kind)) => {
            let result = match row.result.as_deref() {
                Some(raw) => Some(RunResult::parse(raw).map_err(|e| read.corrupt(e.to_string()))?),
                None => None,
            };
            let links = match row.links.as_deref() {
                Some(raw) => {
                    let links: Vec<String> =
                        serde_json::from_str(raw).map_err(|e| read.corrupt(format!("links are not a list: {e}")))?;
                    RunLink::parse_all(&links).map_err(|e| read.corrupt(e.to_string()))?
                }
                None => Vec::new(),
            };
            Some(RunEnded {
                end: RunEnd::from_code(kind).ok_or_else(|| read.corrupt(format!("unknown end {kind:?}")))?,
                at: read.timestamp(at)?,
                result,
                links,
                failure: row.failure.clone(),
            })
        }
        _ => return Err(read.corrupt("ended_at and end_kind disagree".to_owned())),
    };
    Ok(Run {
        id: RunId::from_uuid(read.uuid(&row.id)?),
        item_id: ItemId::from_uuid(read.uuid(&row.item_id)?),
        turn_id: TurnId::from_uuid(read.uuid(&row.turn_id)?),
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        started_at: read.timestamp(row.started_at)?,
        heard_at: read.timestamp(row.heard_at)?,
        from_stage,
        ended,
        cost_micro_usd: row.cost_micro_usd,
        tool_started_at: row.tool_started_at.map(|at| read.timestamp(at)).transpose()?,
        tool_ended_at: row.tool_ended_at.map(|at| read.timestamp(at)).transpose()?,
    })
}
