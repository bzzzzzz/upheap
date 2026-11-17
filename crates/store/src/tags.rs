//! Tags. A tag is owned by a heap or by a team: exactly one of `heap_id` and `team_id` is set, which reading a row
//! checks. Every lookup that comes from a request is scoped by heap, to its own tags and its team's, so a tag ID
//! from another heap or another team finds nothing. Tags are never deleted; there is no delete function, on purpose.

use domain::tag::{Tag, TagDescription, TagName, TagOwner};
use domain::{HeapId, TagId, TeamId, Version};
use sea_orm::sea_query::Query;
use sea_orm::{ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{heaps, tags};

const TABLE: &str = "tags";

/// Inserts a new tag. Fails with [`StoreError::Duplicate`] when its owner already has a tag with that name,
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

/// The tags a heap's tag list holds: the heap's own and its team's.
fn of_heap(heap_id: HeapId) -> Condition {
    let heap = id_to_db(heap_id.as_uuid());
    let team_of_heap = Query::select()
        .column(heaps::Column::TeamId)
        .from(heaps::Entity)
        .and_where(heaps::Column::Id.eq(heap.as_str()))
        .to_owned();
    Condition::any()
        .add(tags::Column::HeapId.eq(heap.as_str()))
        .add(tags::Column::TeamId.in_subquery(team_of_heap))
}

/// Every tag of `heap_id`: its own and its team's, by name.
pub async fn list_tags(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<Tag>, StoreError> {
    tags::Entity::find()
        .filter(of_heap(heap_id))
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
    tags::Entity::find()
        .filter(of_heap(heap_id))
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

/// Whether any heap of the team, archived ones included, has a tag called `name`, ignoring case.
pub async fn has_heap_tag_named_in_team(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    name: &TagName,
) -> Result<bool, StoreError> {
    let heaps_of_team = Query::select()
        .column(heaps::Column::Id)
        .from(heaps::Entity)
        .and_where(heaps::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .to_owned();
    let found = tags::Entity::find()
        .filter(tags::Column::HeapId.in_subquery(heaps_of_team))
        .filter(tags::Column::NameKey.eq(name.to_key()))
        .one(db)
        .await?;
    Ok(found.is_some())
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
