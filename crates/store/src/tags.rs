//! Tags. A tag is owned by a heap or by a team: exactly one of `heap_id` and `team_id` is set, which reading a row
//! checks. Every lookup that comes from a request is scoped by heap, to its own tags and its team's, so a tag ID
//! from another heap or another team finds nothing. Tags are never deleted; there is no delete function, on purpose.
//!
//! A heap tag merged into a team tag keeps its row (`merged_into` is the team tag) so the history still names it, but
//! every function here but [`list_tags_for_history`] leaves it out, as if it were unknown.

use domain::tag::{Tag, TagDescription, TagName, TagOwner};
use domain::{HeapId, TagId, TeamId, Version};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db, version_to_db};
use crate::entities::{heaps, tags};

const TABLE: &str = "tags";

/// Inserts a new tag. Fails with [`StoreError::Duplicate`] when its owner already has a tag with that name,
/// ignoring case.
pub async fn insert_tag(db: &impl ConnectionTrait, tag: &Tag) -> Result<(), StoreError> {
    tags::Entity::insert(to_row(tag)?).exec_without_returning(db).await?;
    Ok(())
}

pub async fn find_tag(db: &impl ConnectionTrait, id: TagId) -> Result<Option<Tag>, StoreError> {
    let row = tags::Entity::find_by_id(id_to_db(id.as_uuid()))
        .filter(tags::Column::MergedInto.is_null())
        .one(db)
        .await?;
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
        .filter(tags::Column::MergedInto.is_null())
        .one(db)
        .await?;
    row.map(to_tag).transpose()
}

/// The tags a heap's tag list holds: the heap's own and its team's. `None` if the heap doesn't exist.
async fn of_heap(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Option<Condition>, StoreError> {
    let heap = id_to_db(heap_id.as_uuid());
    let Some(row) = heaps::Entity::find_by_id(heap.as_str()).one(db).await? else {
        return Ok(None);
    };
    Ok(Some(
        Condition::any()
            .add(tags::Column::HeapId.eq(heap.as_str()))
            .add(tags::Column::TeamId.eq(row.team_id)),
    ))
}

/// Every tag of `heap_id`: its own and its team's, by name.
pub async fn list_tags(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<Tag>, StoreError> {
    let Some(of_heap) = of_heap(db, heap_id).await? else {
        return Ok(Vec::new());
    };
    tags::Entity::find()
        .filter(of_heap)
        .filter(tags::Column::MergedInto.is_null())
        .order_by_asc(tags::Column::NameKey)
        .order_by_asc(tags::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_tag)
        .collect()
}

/// The tags among `ids` that are tags of `heap_id` (its own or its team's), by name. IDs of other heaps' or other
/// teams' tags, or unknown ones, are left out.
pub async fn list_tags_in_heap(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: impl IntoIterator<Item = &TagId>,
) -> Result<Vec<Tag>, StoreError> {
    let ids: Vec<String> = ids.into_iter().map(|id| id_to_db(id.as_uuid())).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some(of_heap) = of_heap(db, heap_id).await? else {
        return Ok(Vec::new());
    };
    tags::Entity::find()
        .filter(of_heap)
        .filter(tags::Column::MergedInto.is_null())
        .filter(tags::Column::Id.is_in(ids))
        .order_by_asc(tags::Column::NameKey)
        .order_by_asc(tags::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_tag)
        .collect()
}

/// The team's tag called `name`, ignoring case.
pub async fn find_team_tag_by_name(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    name: &TagName,
) -> Result<Option<Tag>, StoreError> {
    let row = tags::Entity::find()
        .filter(tags::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .filter(tags::Column::NameKey.eq(name.to_key()))
        .one(db)
        .await?;
    row.map(to_tag).transpose()
}

/// Every tag of the team itself (not of its heaps), by name.
pub async fn list_team_tags(db: &impl ConnectionTrait, team_id: TeamId) -> Result<Vec<Tag>, StoreError> {
    tags::Entity::find()
        .filter(tags::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .order_by_asc(tags::Column::NameKey)
        .order_by_asc(tags::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_tag)
        .collect()
}

/// The tags in use of `heap_ids` called `name`, ignoring case: at most one per heap.
pub async fn list_heap_tags_named(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    name: &TagName,
) -> Result<Vec<Tag>, StoreError> {
    let mut found = Vec::new();
    for chunk in heap_ids.chunks(IN_LIST_CHUNK) {
        let rows = tags::Entity::find()
            .filter(tags::Column::HeapId.is_in(chunk.iter().map(|id| id_to_db(id.as_uuid()))))
            .filter(tags::Column::NameKey.eq(name.to_key()))
            .filter(tags::Column::MergedInto.is_null())
            .all(db)
            .await?;
        for row in rows {
            found.push(to_tag(row)?);
        }
    }
    Ok(found)
}

/// Every tag a heap's history may name: the heap's own, merged ones included, and its team's, by name. The only read
/// that sees merged heap tags.
pub async fn list_tags_for_history(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<Tag>, StoreError> {
    let Some(of_heap) = of_heap(db, heap_id).await? else {
        return Ok(Vec::new());
    };
    tags::Entity::find()
        .filter(of_heap)
        .order_by_asc(tags::Column::NameKey)
        .order_by_asc(tags::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_tag)
        .collect()
}

/// Marks the heap tags `ids` as merged into the team tag `into`, and bumps their versions.
///
/// A merged tag's `name_key` becomes a space and its own ID: that frees its name for a new heap tag (the unique index
/// is on the key), keeps the key unique per row, and can't match any name looked up, since `TagName::parse` trims, so
/// no real key starts with a space. The name and description stay as they were, for the history.
pub async fn mark_merged(db: &impl ConnectionTrait, ids: &[TagId], into: TagId) -> Result<(), StoreError> {
    for id in ids {
        let id = id_to_db(id.as_uuid());
        tags::Entity::update_many()
            .col_expr(tags::Column::MergedInto, Expr::value(id_to_db(into.as_uuid())))
            .col_expr(tags::Column::NameKey, Expr::value(format!(" {id}")))
            .col_expr(tags::Column::Version, Expr::col(tags::Column::Version).add(1))
            .filter(tags::Column::Id.eq(id.as_str()))
            .exec(db)
            .await?;
    }
    Ok(())
}

/// Writes `tag` if the stored row still has version `expected`, and returns it with its new version.
///
/// Returns `None` when the stored version differs. Fails with [`StoreError::Duplicate`] when the new name is
/// taken by another tag of the same owner.
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
    let (heap_id, team_id) = match tag.owner {
        TagOwner::Heap(heap) => (Some(id_to_db(heap.as_uuid())), None),
        TagOwner::Team(team) => (None, Some(id_to_db(team.as_uuid()))),
    };
    Ok(tags::ActiveModel {
        version: Set(version_to_db(tag.version, TABLE, &id)?),
        id: Set(id),
        heap_id: Set(heap_id),
        team_id: Set(team_id),
        name: Set(tag.name.as_str().to_owned()),
        name_key: Set(tag.name.to_key()),
        description: Set(tag.description.as_ref().map(|d| d.as_str().to_owned())),
        created_at: Set(tag.created_at.as_millis()),
        // A tag written through the domain type is in use; `mark_merged` is what merges one.
        merged_into: Set(None),
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
    let owner = match (row.heap_id.as_deref(), row.team_id.as_deref()) {
        (Some(heap), None) => TagOwner::Heap(HeapId::from_uuid(read.uuid(heap)?)),
        (None, Some(team)) => TagOwner::Team(TeamId::from_uuid(read.uuid(team)?)),
        _ => return Err(read.corrupt("exactly one of heap_id and team_id must be set".to_owned())),
    };
    Ok(Tag {
        id: TagId::from_uuid(read.uuid(&row.id)?),
        owner,
        name: TagName::parse(&row.name).map_err(|e| read.corrupt(e.to_string()))?,
        description,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
