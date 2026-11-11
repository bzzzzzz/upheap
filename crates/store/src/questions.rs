//! An item's questions. Each is its own row with its own version, so replying to one never touches the item's
//! version. Questions are never deleted: an answered one keeps its row, with its reply.

use domain::item::StateKind;
use domain::question::{
    Choices, Question, QuestionKind, QuestionNumber, QuestionState, QuestionText, Reply, Withdrawal,
};
use domain::step::StepNumber;
use domain::text::Context;
use domain::validation::Field;
use domain::{HeapId, ItemId, QuestionId, UserId, Version};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect,
};
use std::collections::HashMap;

use crate::StoreError;
use crate::codes::{GroupCount, IN_LIST_CHUNK, RowReader, id_to_db, version_to_db};
use crate::entities::{item_questions, items};

const TABLE: &str = "item_questions";

/// Inserts a new question.
pub async fn insert_question(db: &impl ConnectionTrait, question: &Question) -> Result<(), StoreError> {
    item_questions::Entity::insert(to_row(question)?)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// The questions of `item_id` in number order.
pub async fn list_questions(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Vec<Question>, StoreError> {
    read_questions(db, item_id, false).await
}

/// The questions of `item_id` in number order, read with a locking read (`FOR SHARE` on MySQL and Postgres), which
/// sees the latest committed rows rather than the transaction's snapshot. A question write calls it after
/// [`crate::heaps::lock_heap`], so on MySQL (REPEATABLE READ) it doesn't work from the snapshot fixed by an earlier
/// plain read: two racing questions then can't pick the same number, and the cap of
/// [`domain::question::MAX_QUESTIONS`] holds. SQLite has no row locks and needs none (see `lock_heap`).
pub async fn list_questions_locking(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Vec<Question>, StoreError> {
    read_questions(db, item_id, true).await
}

/// A question row with the heap of its item.
#[derive(sea_orm::FromQueryResult)]
struct QuestionInHeap {
    #[sea_orm(nested)]
    question: item_questions::Model,
    heap_id: String,
}

/// The open questions on the open items (open, in progress, blocked) of `heap_ids`, each with the heap of its item, in
/// one query per [`IN_LIST_CHUNK`] heaps. Questions of closed items and answered ones are left out; the order is the
/// database's, so the caller orders them. The caller leaves archived heaps out of `heap_ids`.
pub async fn list_open_questions_on_open_items(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
) -> Result<Vec<(HeapId, Question)>, StoreError> {
    let ids: Vec<String> = heap_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut found = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_questions::Entity::find()
            .join(
                JoinType::InnerJoin,
                item_questions::Entity::belongs_to(items::Entity)
                    .from(item_questions::Column::ItemId)
                    .to(items::Column::Id)
                    .into(),
            )
            .filter(items::Column::HeapId.is_in(chunk.iter().map(String::as_str)))
            .filter(items::Column::State.is_in(StateKind::OPEN.map(StateKind::code)))
            .filter(item_questions::Column::State.eq(QuestionState::Open.code()))
            // Only the item's heap is read from the join: the item itself, with its context, isn't needed.
            .column_as(items::Column::HeapId, "heap_id")
            .order_by_asc(item_questions::Column::ItemId)
            .order_by_asc(item_questions::Column::Number)
            .into_model::<QuestionInHeap>()
            .all(db)
            .await?;
        for QuestionInHeap { question, heap_id } in rows {
            let read = RowReader {
                table: TABLE,
                id: &question.id,
            };
            let heap_id = HeapId::from_uuid(read.uuid(&heap_id)?);
            found.push((heap_id, to_question(question)?));
        }
    }
    Ok(found)
}

/// How many open questions each of `item_ids` has, in one grouped query per [`IN_LIST_CHUNK`] items, without loading
/// them. Items with none are left out.
pub async fn count_open_questions(
    db: &impl ConnectionTrait,
    item_ids: &[ItemId],
) -> Result<HashMap<ItemId, u32>, StoreError> {
    let ids: Vec<String> = item_ids.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let mut counts = HashMap::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = item_questions::Entity::find()
            .select_only()
            .column_as(item_questions::Column::ItemId, "group_key")
            .column_as(item_questions::Column::Id.count(), "count")
            .filter(item_questions::Column::ItemId.is_in(chunk.iter().map(String::as_str)))
            .filter(item_questions::Column::State.eq(QuestionState::Open.code()))
            .group_by(item_questions::Column::ItemId)
            .into_model::<GroupCount>()
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: TABLE,
                id: &row.group_key,
            };
            counts.insert(ItemId::from_uuid(read.uuid(&row.group_key)?), row.count);
        }
    }
    Ok(counts)
}

/// [`list_questions`], or with `lock` [`list_questions_locking`].
async fn read_questions(db: &impl ConnectionTrait, item_id: ItemId, lock: bool) -> Result<Vec<Question>, StoreError> {
    let query = item_questions::Entity::find()
        .filter(item_questions::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .order_by_asc(item_questions::Column::Number);
    let rows = crate::items::shared_if(query, lock).all(db).await?;
    rows.into_iter().map(to_question).collect()
}

/// Writes `question`'s reply or withdrawal if the stored row still has version `expected`, and returns it with its
/// new version.
///
/// Returns `None` when the stored version differs, meaning someone else wrote first; nothing is written then. What
/// the asker wrote never changes, so only the state, the reply and the withdrawal are written.
pub async fn update_question(
    db: &impl ConnectionTrait,
    question: &Question,
    expected: Version,
) -> Result<Option<Question>, StoreError> {
    let saved = Question {
        version: expected.next(),
        ..question.clone()
    };
    let id = id_to_db(question.id.as_uuid());
    let reply = saved.reply.as_ref();
    let withdrawal = saved.withdrawal.as_ref();
    let result = item_questions::Entity::update_many()
        .set(item_questions::ActiveModel {
            state: Set(saved.state().code().to_owned()),
            reply_picked: Set(reply.map(|reply| picked_to_db(&reply.picked))),
            reply_comment: Set(reply.and_then(|reply| reply.comment.as_ref().map(|c| c.as_str().to_owned()))),
            replied_by: Set(reply
                .and_then(|reply| reply.replied_by)
                .map(|user| id_to_db(user.as_uuid()))),
            replied_at: Set(reply.map(|reply| reply.replied_at.as_millis())),
            withdrawn_by: Set(withdrawal
                .and_then(|withdrawal| withdrawal.withdrawn_by)
                .map(|user| id_to_db(user.as_uuid()))),
            withdrawn_at: Set(withdrawal.map(|withdrawal| withdrawal.withdrawn_at.as_millis())),
            version: Set(version_to_db(saved.version, TABLE, &id)?),
            ..Default::default()
        })
        .filter(item_questions::Column::Id.eq(id.as_str()))
        .filter(item_questions::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
}

fn picked_to_db(picked: &[u32]) -> String {
    serde_json::to_string(picked).unwrap_or_else(|_| "[]".to_owned())
}

fn to_row(question: &Question) -> Result<item_questions::ActiveModel, StoreError> {
    let id = id_to_db(question.id.as_uuid());
    let number =
        i32::try_from(question.number.get()).map_err(|_| StoreError::corrupt(TABLE, &id, "number overflow"))?;
    let step_number = question
        .step
        .map(|step| i32::try_from(step.get()).map_err(|_| StoreError::corrupt(TABLE, &id, "step number overflow")))
        .transpose()?;
    let reply = question.reply.as_ref();
    let withdrawal = question.withdrawal.as_ref();
    Ok(item_questions::ActiveModel {
        version: Set(version_to_db(question.version, TABLE, &id)?),
        id: Set(id),
        item_id: Set(id_to_db(question.item_id.as_uuid())),
        number: Set(number),
        kind: Set(question.kind.code().to_owned()),
        text: Set(question.text.as_str().to_owned()),
        details: Set(question.details.as_ref().map(|details| details.as_str().to_owned())),
        choices: Set(serde_json::to_string(question.choices.labels()).unwrap_or_else(|_| "[]".to_owned())),
        many: Set(question.many),
        step_number: Set(step_number),
        state: Set(question.state().code().to_owned()),
        asked_by: Set(question.asked_by.map(|user| id_to_db(user.as_uuid()))),
        asked_at: Set(question.asked_at.as_millis()),
        reply_picked: Set(reply.map(|reply| picked_to_db(&reply.picked))),
        reply_comment: Set(reply.and_then(|reply| reply.comment.as_ref().map(|c| c.as_str().to_owned()))),
        replied_by: Set(reply
            .and_then(|reply| reply.replied_by)
            .map(|user| id_to_db(user.as_uuid()))),
        replied_at: Set(reply.map(|reply| reply.replied_at.as_millis())),
        withdrawn_by: Set(withdrawal
            .and_then(|withdrawal| withdrawal.withdrawn_by)
            .map(|user| id_to_db(user.as_uuid()))),
        withdrawn_at: Set(withdrawal.map(|withdrawal| withdrawal.withdrawn_at.as_millis())),
    })
}

fn to_question(row: item_questions::Model) -> Result<Question, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let number = u32::try_from(row.number)
        .ok()
        .and_then(QuestionNumber::new)
        .ok_or_else(|| read.corrupt(format!("number {} is not positive", row.number)))?;
    let kind =
        QuestionKind::from_code(&row.kind).ok_or_else(|| read.corrupt(format!("unknown kind {:?}", row.kind)))?;
    let state =
        QuestionState::from_code(&row.state).ok_or_else(|| read.corrupt(format!("unknown state {:?}", row.state)))?;
    let details = match row.details.as_deref() {
        Some(raw) => Context::parse_as(raw, Field::Details).map_err(|e| read.corrupt(e.to_string()))?,
        None => None,
    };
    let choices: Vec<String> =
        serde_json::from_str(&row.choices).map_err(|e| read.corrupt(format!("choices are not a list: {e}")))?;
    let step = row
        .step_number
        .map(|step| {
            u32::try_from(step)
                .ok()
                .and_then(StepNumber::new)
                .ok_or_else(|| read.corrupt(format!("step number {step} is not positive")))
        })
        .transpose()?;
    let withdrawal = match (state, row.withdrawn_at) {
        (QuestionState::Withdrawn, Some(withdrawn_at)) => Some(Withdrawal {
            withdrawn_by: row
                .withdrawn_by
                .as_deref()
                .map(|user| read.uuid(user).map(UserId::from_uuid))
                .transpose()?,
            withdrawn_at: read.timestamp(withdrawn_at)?,
        }),
        (QuestionState::Withdrawn, None) | (_, Some(_)) => {
            return Err(read.corrupt("the state and the withdrawal disagree".to_owned()));
        }
        (_, None) => None,
    };
    let reply = match (state, row.reply_picked.as_deref(), row.replied_at) {
        (QuestionState::Open | QuestionState::Withdrawn, None, None) => None,
        (QuestionState::Answered, Some(picked), Some(replied_at)) => {
            let comment = match row.reply_comment.as_deref() {
                Some(raw) => Context::parse_as(raw, Field::Comment).map_err(|e| read.corrupt(e.to_string()))?,
                None => None,
            };
            Some(Reply {
                picked: serde_json::from_str(picked).map_err(|e| read.corrupt(format!("picks are not a list: {e}")))?,
                comment,
                replied_by: row
                    .replied_by
                    .as_deref()
                    .map(|user| read.uuid(user).map(UserId::from_uuid))
                    .transpose()?,
                replied_at: read.timestamp(replied_at)?,
            })
        }
        _ => return Err(read.corrupt("the state and the reply disagree".to_owned())),
    };
    Ok(Question {
        id: QuestionId::from_uuid(read.uuid(&row.id)?),
        item_id: ItemId::from_uuid(read.uuid(&row.item_id)?),
        number,
        kind,
        text: QuestionText::parse(&row.text).map_err(|e| read.corrupt(e.to_string()))?,
        details,
        choices: Choices::from_stored(choices),
        many: row.many,
        step,
        asked_by: row
            .asked_by
            .as_deref()
            .map(|user| read.uuid(user).map(UserId::from_uuid))
            .transpose()?,
        asked_at: read.timestamp(row.asked_at)?,
        reply,
        withdrawal,
        version: read.version(row.version)?,
    })
}
