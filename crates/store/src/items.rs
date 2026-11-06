//! Items. Every query is scoped by heap where the caller knows the heap.

use domain::item::{Description, Due, Item, ItemState, Size, StateKind, Title};
use domain::{HeapId, ItemId, Timestamp, Version};
use sea_orm::sea_query::Condition;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::items;

const TABLE: &str = "items";

/// Position after which a page of closed items starts: the last item of the previous page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedCursor {
    pub closed_at: Timestamp,
    pub id: ItemId,
}

pub async fn insert_item(db: &impl ConnectionTrait, item: &Item) -> Result<(), StoreError> {
    items::Entity::insert(to_row(item)?).exec_without_returning(db).await?;
    Ok(())
}

pub async fn find_item(db: &impl ConnectionTrait, id: ItemId) -> Result<Option<Item>, StoreError> {
    let row = items::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_item).transpose()
}

/// Every item of `heap_id` in one of `states`, oldest first.
pub async fn list_items_in_states(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    states: &[StateKind],
) -> Result<Vec<Item>, StoreError> {
    items::Entity::find()
        .filter(items::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(items::Column::State.is_in(states.iter().map(|s| s.code())))
        .order_by_asc(items::Column::CreatedAt)
        .order_by_asc(items::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect()
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
    query
        .order_by_desc(items::Column::ClosedAt)
        .order_by_desc(items::Column::Id)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(to_item)
        .collect()
}

/// Writes `item` if the stored row still has version `expected`, and returns it with its new version.
///
/// Returns `None` when the stored version differs, meaning someone else wrote first.
pub async fn update_item(
    db: &impl ConnectionTrait,
    item: &Item,
    expected: Version,
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
    Ok((result.rows_affected == 1).then_some(saved))
}

fn to_row(item: &Item) -> Result<items::ActiveModel, StoreError> {
    let id = id_to_db(item.id.as_uuid());
    Ok(items::ActiveModel {
        version: Set(version_to_db(item.version, TABLE, &id)?),
        id: Set(id),
        heap_id: Set(id_to_db(item.heap_id.as_uuid())),
        title: Set(item.title.as_str().to_owned()),
        description: Set(item.description.as_ref().map(|d| d.as_str().to_owned())),
        state: Set(item.state.kind().code().to_owned()),
        effort: Set(item.effort.map(|s| s.code().to_owned())),
        impact: Set(item.impact.map(|s| s.code().to_owned())),
        due_at: Set(item.due.map(|d| d.at.as_millis())),
        due_firm: Set(item.due.map(|d| d.firm)),
        created_at: Set(item.created_at.as_millis()),
        closed_at: Set(item.state.closed_at().map(Timestamp::as_millis)),
    })
}

fn to_item(row: items::Model) -> Result<Item, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let kind =
        StateKind::from_code(&row.state).ok_or_else(|| read.corrupt(format!("unknown state {:?}", row.state)))?;
    let state = ItemState::from_parts(kind, read.optional_timestamp(row.closed_at)?)
        .map_err(|e| read.corrupt(e.to_string()))?;
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
        title: Title::parse(&row.title).map_err(|e| read.corrupt(e.to_string()))?,
        description,
        state,
        effort: size(row.effort)?,
        impact: size(row.impact)?,
        due,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
