//! Custom parameters with their options and tag links. Parameters and options are archived, never deleted, and
//! tag links are marked removed; there is no delete function, on purpose.
//!
//! A parameter is read and written whole: its row, its options and its tag links. Every function that returns a
//! parameter returns all three.

use std::collections::{BTreeSet, HashMap};

use domain::parameter::{ArchivedOption, OptionLabel, Parameter, ParameterName, ParameterOption, Weight};
use domain::{HeapId, ItemId, OptionId, ParameterId, TagId, Timestamp, Version};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, Select};

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db, version_to_db};
use crate::entities::{item_parameter_values, items, parameter_options, parameter_tags, parameters};

const TABLE: &str = "parameters";

/// How a read locks the rows it reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lock {
    None,
    /// `FOR SHARE` on MySQL and Postgres: sees the latest committed rows, not the transaction's snapshot.
    Share,
    /// `FOR UPDATE` on the parameter row, `FOR SHARE` on its options and tag links.
    Update,
}

fn locked<E: EntityTrait>(query: Select<E>, lock: Lock) -> Select<E> {
    match lock {
        Lock::None => query,
        Lock::Share => query.lock_shared(),
        Lock::Update => query.lock_exclusive(),
    }
}

/// The lock for a parameter's options and tag links when the parameter row is read with `lock`.
const fn child_lock(lock: Lock) -> Lock {
    match lock {
        Lock::None => Lock::None,
        Lock::Share | Lock::Update => Lock::Share,
    }
}

/// Inserts a new parameter with its options and tag links. The caller has checked the tags belong to its heap.
pub async fn insert_parameter(db: &impl ConnectionTrait, parameter: &Parameter) -> Result<(), StoreError> {
    let id = id_to_db(parameter.id.as_uuid());
    let row = parameters::ActiveModel {
        version: Set(version_to_db(parameter.version, TABLE, &id)?),
        id: Set(id),
        heap_id: Set(id_to_db(parameter.heap_id.as_uuid())),
        name: Set(parameter.name.as_str().to_owned()),
        weight: Set(i32::from(parameter.weight.get())),
        created_at: Set(parameter.created_at.as_millis()),
        archived_at: Set(parameter.archived_at.map(Timestamp::as_millis)),
    };
    parameters::Entity::insert(row).exec_without_returning(db).await?;
    sync_options(db, parameter).await?;
    sync_tags(db, parameter.id, &parameter.tags, parameter.created_at).await
}

pub async fn find_parameter(db: &impl ConnectionTrait, id: ParameterId) -> Result<Option<Parameter>, StoreError> {
    load_one(db, id, Lock::None).await
}

/// Reads the parameter and locks its row until the transaction ends. Its options and tag links are read with a
/// locking read too, so they are the latest committed ones even on MySQL.
///
/// Callers lock the heap first, as every heap write does, so the order of locks is always heap, then parameter.
pub async fn lock_parameter(db: &impl ConnectionTrait, id: ParameterId) -> Result<Option<Parameter>, StoreError> {
    load_one(db, id, Lock::Update).await
}

/// The parameter an option belongs to, or `None` for an unknown option.
pub async fn find_option_parameter(
    db: &impl ConnectionTrait,
    option_id: OptionId,
) -> Result<Option<ParameterId>, StoreError> {
    let Some(row) = parameter_options::Entity::find_by_id(id_to_db(option_id.as_uuid()))
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    let read = RowReader {
        table: "parameter_options",
        id: &row.id,
    };
    Ok(Some(ParameterId::from_uuid(read.uuid(&row.parameter_id)?)))
}

/// Every parameter of `heap_id`, archived ones included, by name ignoring case, then by ID.
///
/// Sorted here rather than in SQL, because MySQL and SQLite order text differently by default.
pub async fn list_parameters(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<Parameter>, StoreError> {
    load_heap(db, heap_id, Lock::None).await
}

/// [`list_parameters`] with a locking read, for item writes that check values against the parameters after
/// locking the heap: on MySQL a plain read would see the snapshot from before the lock was granted.
pub async fn list_parameters_locking(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<Parameter>, StoreError> {
    load_heap(db, heap_id, Lock::Share).await
}

/// Writes `parameter` with its options and tag links if the stored row still has version `expected`, and returns
/// it with its new version. Tags it no longer has are marked removed at `now`.
///
/// Returns `None` when the stored version differs; nothing is written then.
pub async fn update_parameter(
    db: &impl ConnectionTrait,
    parameter: &Parameter,
    expected: Version,
    now: Timestamp,
) -> Result<Option<Parameter>, StoreError> {
    let saved = Parameter {
        version: expected.next(),
        ..parameter.clone()
    };
    let id = id_to_db(parameter.id.as_uuid());
    let row = parameters::ActiveModel {
        name: Set(saved.name.as_str().to_owned()),
        weight: Set(i32::from(saved.weight.get())),
        archived_at: Set(saved.archived_at.map(Timestamp::as_millis)),
        version: Set(version_to_db(saved.version, TABLE, &id)?),
        ..Default::default()
    };
    let result = parameters::Entity::update_many()
        .set(row)
        .filter(parameters::Column::Id.eq(id.as_str()))
        .filter(parameters::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    if result.rows_affected != 1 {
        return Ok(None);
    }
    sync_options(db, &saved).await?;
    sync_tags(db, saved.id, &saved.tags, now).await?;
    Ok(Some(saved))
}

/// The items of `heap_id` that aren't closed and have a value for `parameter_id`, by ID.
///
/// A locking read, so it sees values written while the caller waited for the heap lock.
pub async fn list_open_items_with_value(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    parameter_id: ParameterId,
) -> Result<Vec<ItemId>, StoreError> {
    let rows = item_parameter_values::Entity::find()
        .filter(item_parameter_values::Column::ParameterId.eq(id_to_db(parameter_id.as_uuid())))
        .filter(item_parameter_values::Column::OptionId.is_not_null())
        .lock_shared()
        .all(db)
        .await?;
    open_items_among(db, heap_id, rows.iter().map(|row| row.item_id.as_str())).await
}

/// Switches every item of `heap_id` that isn't closed from option `from` to option `to`, and bumps each switched
/// item's version, so an edit made from the old value fails instead of overwriting the switch. Closed items keep
/// `from`. Returns the switched items, by ID.
pub async fn switch_open_items(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    from: OptionId,
    to: OptionId,
) -> Result<Vec<ItemId>, StoreError> {
    let from = id_to_db(from.as_uuid());
    let rows = item_parameter_values::Entity::find()
        .filter(item_parameter_values::Column::OptionId.eq(from.as_str()))
        .lock_exclusive()
        .all(db)
        .await?;
    let switched = open_items_among(db, heap_id, rows.iter().map(|row| row.item_id.as_str())).await?;
    let ids: Vec<String> = switched.iter().map(|id| id_to_db(id.as_uuid())).collect();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        item_parameter_values::Entity::update_many()
            .col_expr(item_parameter_values::Column::OptionId, id_to_db(to.as_uuid()).into())
            .filter(item_parameter_values::Column::OptionId.eq(from.as_str()))
            .filter(item_parameter_values::Column::ItemId.is_in(chunk.iter().map(String::as_str)))
            .exec(db)
            .await?;
        items::Entity::update_many()
            .col_expr(items::Column::Version, Expr::col(items::Column::Version).add(1))
            .filter(items::Column::Id.is_in(chunk.iter().map(String::as_str)))
            .exec(db)
            .await?;
    }
    Ok(switched)
}

/// The items among `ids` that belong to `heap_id` and aren't closed, by ID. A locking read.
async fn open_items_among<'a>(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: impl Iterator<Item = &'a str>,
) -> Result<Vec<ItemId>, StoreError> {
    let ids: Vec<&str> = ids.collect();
    let mut open = Vec::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = items::Entity::find()
            .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
            .filter(items::Column::Id.is_in(chunk.iter().copied()))
            .filter(items::Column::ClosedAt.is_null())
            .lock_shared()
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: "items",
                id: &row.id,
            };
            open.push(ItemId::from_uuid(read.uuid(&row.id)?));
        }
    }
    open.sort();
    Ok(open)
}

/// Writes every option of `parameter` as it is now: options in use at their position, archived ones with their
/// replacement. Options without a row get one.
async fn sync_options(db: &impl ConnectionTrait, parameter: &Parameter) -> Result<(), StoreError> {
    let parameter_id = id_to_db(parameter.id.as_uuid());
    let existing: BTreeSet<String> = parameter_options::Entity::find()
        .filter(parameter_options::Column::ParameterId.eq(parameter_id.as_str()))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.id)
        .collect();
    let in_use = parameter
        .options
        .iter()
        .zip(0_i32..)
        .map(|(option, position)| (option.id, &option.label, Some(position), None));
    let archived = parameter.archived_options.iter().map(|option| {
        (
            option.id,
            &option.label,
            None,
            Some((option.archived_at, option.replaced_by)),
        )
    });
    // Options in use first, so a replacement's row exists before the archived option that points to it.
    for (id, label, position, archive) in in_use.chain(archived) {
        let id = id_to_db(id.as_uuid());
        let mut row = parameter_options::ActiveModel {
            id: Set(id.clone()),
            parameter_id: Set(parameter_id.clone()),
            label: Set(label.as_str().to_owned()),
            archived_at: Set(archive.map(|(at, _)| at.as_millis())),
            replaced_by_option_id: Set(archive.map(|(_, by)| id_to_db(by.as_uuid()))),
            ..Default::default()
        };
        if let Some(position) = position {
            row.position = Set(position);
        }
        if existing.contains(&id) {
            row.id = sea_orm::ActiveValue::NotSet;
            row.parameter_id = sea_orm::ActiveValue::NotSet;
            parameter_options::Entity::update_many()
                .set(row)
                .filter(parameter_options::Column::Id.eq(id.as_str()))
                .exec(db)
                .await?;
        } else {
            row.position = Set(position.unwrap_or(0));
            parameter_options::Entity::insert(row)
                .exec_without_returning(db)
                .await?;
        }
    }
    Ok(())
}

/// Makes the parameter's active tag links exactly `tags`: adds the missing ones and marks the others removed.
async fn sync_tags(
    db: &impl ConnectionTrait,
    parameter_id: ParameterId,
    tags: &BTreeSet<TagId>,
    now: Timestamp,
) -> Result<(), StoreError> {
    let parameter = id_to_db(parameter_id.as_uuid());
    let rows = parameter_tags::Entity::find()
        .filter(parameter_tags::Column::ParameterId.eq(parameter.as_str()))
        .all(db)
        .await?;
    let wanted: BTreeSet<String> = tags.iter().map(|id| id_to_db(id.as_uuid())).collect();
    let removed: Vec<&str> = rows
        .iter()
        .filter(|row| row.removed_at.is_none() && !wanted.contains(&row.tag_id))
        .map(|row| row.tag_id.as_str())
        .collect();
    if !removed.is_empty() {
        parameter_tags::Entity::update_many()
            .col_expr(parameter_tags::Column::RemovedAt, now.as_millis().into())
            .filter(parameter_tags::Column::ParameterId.eq(parameter.as_str()))
            .filter(parameter_tags::Column::TagId.is_in(removed))
            .exec(db)
            .await?;
    }
    let known: HashMap<&str, bool> = rows
        .iter()
        .map(|row| (row.tag_id.as_str(), row.removed_at.is_some()))
        .collect();
    for tag in &wanted {
        match known.get(tag.as_str()) {
            Some(false) => {}
            Some(true) => {
                parameter_tags::Entity::update_many()
                    .col_expr(parameter_tags::Column::AddedAt, now.as_millis().into())
                    .col_expr(parameter_tags::Column::RemovedAt, Option::<i64>::None.into())
                    .filter(parameter_tags::Column::ParameterId.eq(parameter.as_str()))
                    .filter(parameter_tags::Column::TagId.eq(tag.as_str()))
                    .exec(db)
                    .await?;
            }
            None => {
                let row = parameter_tags::ActiveModel {
                    parameter_id: Set(parameter.clone()),
                    tag_id: Set(tag.clone()),
                    added_at: Set(now.as_millis()),
                    removed_at: Set(None),
                };
                parameter_tags::Entity::insert(row).exec_without_returning(db).await?;
            }
        }
    }
    Ok(())
}

async fn load_one(db: &impl ConnectionTrait, id: ParameterId, lock: Lock) -> Result<Option<Parameter>, StoreError> {
    let query = parameters::Entity::find_by_id(id_to_db(id.as_uuid()));
    let Some(row) = locked(query, lock).one(db).await? else {
        return Ok(None);
    };
    Ok(assemble(db, vec![row], child_lock(lock)).await?.pop())
}

async fn load_heap(db: &impl ConnectionTrait, heap_id: HeapId, lock: Lock) -> Result<Vec<Parameter>, StoreError> {
    let query = parameters::Entity::find().filter(parameters::Column::HeapId.eq(id_to_db(heap_id.as_uuid())));
    let rows = locked(query, lock).all(db).await?;
    let mut parameters = assemble(db, rows, child_lock(lock)).await?;
    parameters.sort_by_cached_key(|parameter| (parameter.name.to_key(), parameter.id));
    Ok(parameters)
}

/// The parameters of `rows`, with their options and tag links read with `lock`.
async fn assemble(
    db: &impl ConnectionTrait,
    rows: Vec<parameters::Model>,
    lock: Lock,
) -> Result<Vec<Parameter>, StoreError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let options = locked(
        parameter_options::Entity::find().filter(parameter_options::Column::ParameterId.is_in(ids.iter().copied())),
        lock,
    )
    .all(db)
    .await?;
    let links = locked(
        parameter_tags::Entity::find()
            .filter(parameter_tags::Column::ParameterId.is_in(ids.iter().copied()))
            .filter(parameter_tags::Column::RemovedAt.is_null()),
        lock,
    )
    .all(db)
    .await?;

    let mut options_of: HashMap<&str, Vec<&parameter_options::Model>> = HashMap::new();
    for option in &options {
        options_of.entry(option.parameter_id.as_str()).or_default().push(option);
    }
    let mut tags_of: HashMap<&str, Vec<&parameter_tags::Model>> = HashMap::new();
    for link in &links {
        tags_of.entry(link.parameter_id.as_str()).or_default().push(link);
    }
    rows.iter()
        .map(|row| {
            to_parameter(
                row,
                options_of.remove(row.id.as_str()).unwrap_or_default(),
                tags_of.remove(row.id.as_str()).unwrap_or_default(),
            )
        })
        .collect()
}

fn to_parameter(
    row: &parameters::Model,
    mut options: Vec<&parameter_options::Model>,
    links: Vec<&parameter_tags::Model>,
) -> Result<Parameter, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    options.sort_by(|a, b| (a.position, &a.id).cmp(&(b.position, &b.id)));
    let mut in_use = Vec::new();
    let mut archived = Vec::new();
    for option in options {
        let id = OptionId::from_uuid(read.uuid(&option.id)?);
        let label = OptionLabel::parse(&option.label).map_err(|e| read.corrupt(format!("option label: {e}")))?;
        match (option.archived_at, option.replaced_by_option_id.as_deref()) {
            (None, None) => in_use.push(ParameterOption { id, label }),
            (Some(at), Some(by)) => archived.push(ArchivedOption {
                id,
                label,
                archived_at: read.timestamp(at)?,
                replaced_by: OptionId::from_uuid(read.uuid(by)?),
            }),
            _ => {
                return Err(read.corrupt(format!(
                    "option {} must have archived_at and replaced_by_option_id together",
                    option.id
                )));
            }
        }
    }
    archived.sort_by_key(|option| (option.archived_at, option.id));
    let tags = links
        .iter()
        .map(|link| read.uuid(&link.tag_id).map(TagId::from_uuid))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let weight = Weight::parse(row.weight.into())
        .ok()
        .ok_or_else(|| read.corrupt(format!("weight {} out of range", row.weight)))?;
    Ok(Parameter {
        id: ParameterId::from_uuid(read.uuid(&row.id)?),
        heap_id: HeapId::from_uuid(read.uuid(&row.heap_id)?),
        name: ParameterName::parse(&row.name).map_err(|e| read.corrupt(e.to_string()))?,
        weight,
        options: in_use,
        archived_options: archived,
        tags,
        created_at: read.timestamp(row.created_at)?,
        archived_at: read.optional_timestamp(row.archived_at)?,
        version: read.version(row.version)?,
    })
}
