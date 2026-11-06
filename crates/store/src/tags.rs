//! Tags. Every lookup that comes from a request is scoped by heap, so a tag ID from another heap finds nothing.
//! Tags are never deleted; there is no delete function, on purpose.

use domain::tag::{Tag, TagDescription, TagName};
use domain::{HeapId, TagId, Version};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::tags;

const TABLE: &str = "tags";

/// Inserts a new tag. Fails with [`StoreError::Duplicate`] when the heap already has a tag with that name,
/// ignoring case.
pub async fn insert_tag(db: &impl ConnectionTrait, tag: &Tag) -> Result<(), StoreError> {
    tags::Entity::insert(to_row(tag)?).exec_without_returning(db).await?;
    Ok(())
}

pub async fn find_tag(db: &impl ConnectionTrait, id: TagId) -> Result<Option<Tag>, StoreError> {
    let row = tags::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_tag).transpose()
}

/// The heap's tag called `name`, ignoring case.
pub async fn find_tag_by_name(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    name: &TagName,
) -> Result<Option<Tag>, StoreError> {
    let row = tags::Entity::find()
        .filter(tags::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(tags::Column::NameKey.eq(name.to_key()))
        .one(db)
        .await?;
    row.map(to_tag).transpose()
}

/// Every tag of `heap_id`, by name.
pub async fn list_tags(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<Tag>, StoreError> {
    tags::Entity::find()
        .filter(tags::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .order_by_asc(tags::Column::NameKey)
        .order_by_asc(tags::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_tag)
        .collect()
}

/// The tags among `ids` that belong to `heap_id`, by name. IDs from other heaps, or unknown ones, are left out.
pub async fn list_tags_in_heap(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: impl IntoIterator<Item = &TagId>,
) -> Result<Vec<Tag>, StoreError> {
    let ids: Vec<String> = ids.into_iter().map(|id| id_to_db(id.as_uuid())).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    tags::Entity::find()
        .filter(tags::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(tags::Column::Id.is_in(ids))
        .order_by_asc(tags::Column::NameKey)
        .order_by_asc(tags::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_tag)
        .collect()
}

/// Writes `tag` if the stored row still has version `expected`, and returns it with its new version.
///
/// Returns `None` when the stored version differs. Fails with [`StoreError::Duplicate`] when the new name is
/// taken in the heap.
pub async fn update_tag(db: &impl ConnectionTrait, tag: &Tag, expected: Version) -> Result<Option<Tag>, StoreError> {
    let saved = Tag {
        version: expected.next(),
        ..tag.clone()
    };
    let mut row = to_row(&saved)?;
    row.id = sea_orm::ActiveValue::NotSet;
    let id = id_to_db(tag.id.as_uuid());
    let result = tags::Entity::update_many()
        .set(row)
        .filter(tags::Column::Id.eq(id.as_str()))
        .filter(tags::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
}

fn to_row(tag: &Tag) -> Result<tags::ActiveModel, StoreError> {
    let id = id_to_db(tag.id.as_uuid());
    Ok(tags::ActiveModel {
        version: Set(version_to_db(tag.version, TABLE, &id)?),
        id: Set(id),
        heap_id: Set(id_to_db(tag.heap_id.as_uuid())),
        name: Set(tag.name.as_str().to_owned()),
        name_key: Set(tag.name.to_key()),
        description: Set(tag.description.as_ref().map(|d| d.as_str().to_owned())),
        created_at: Set(tag.created_at.as_millis()),
    })
}

fn to_tag(row: tags::Model) -> Result<Tag, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let description = match row.description.as_deref() {
        Some(raw) => TagDescription::parse(raw).map_err(|e| read.corrupt(e.to_string()))?,
        None => None,
    };
    Ok(Tag {
        id: TagId::from_uuid(read.uuid(&row.id)?),
        heap_id: HeapId::from_uuid(read.uuid(&row.heap_id)?),
        name: TagName::parse(&row.name).map_err(|e| read.corrupt(e.to_string()))?,
        description,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
