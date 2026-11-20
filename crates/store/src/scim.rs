//! What the identity provider pushed over SCIM: its users, its groups and who is in them. The rows mirror the
//! provider's data, so removing one deletes it.

use std::collections::BTreeSet;

use domain::Timestamp;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde_json::Value;
use uuid::Uuid;

use crate::StoreError;
use crate::codes::{IN_LIST_CHUNK, RowReader, id_to_db};
use crate::entities::{scim_group_members, scim_groups, scim_users};

/// A user the provider pushed.
#[derive(Clone, Debug, PartialEq)]
pub struct ScimUser {
    pub id: Uuid,
    pub user_name: String,
    pub external_id: Option<String>,
    pub active: bool,
    /// The resource as sent, less the attributes the fields above hold: a JSON object.
    pub body: Value,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A group the provider pushed. Its members are in [`list_group_members`].
#[derive(Clone, Debug, PartialEq)]
pub struct ScimGroup {
    pub id: Uuid,
    pub display_name: String,
    pub external_id: Option<String>,
    /// The resource as sent, less the attributes the fields above hold and the members: a JSON object.
    pub body: Value,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The attribute a user list is filtered on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserField {
    UserName,
    ExternalId,
}

/// The attribute a group list is filtered on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupField {
    DisplayName,
    ExternalId,
}

/// One page of a list: how many rows match, and the rows from the page's offset.
#[derive(Debug)]
pub struct Page<T> {
    pub total: u64,
    pub rows: Vec<T>,
}

fn body_to_string(table: &'static str, id: Uuid, body: &Value) -> Result<String, StoreError> {
    serde_json::to_string(body)
        .map_err(|e| StoreError::corrupt(table, &id.to_string(), format!("body can't be written: {e}")))
}

fn user_from_row(row: scim_users::Model) -> Result<ScimUser, StoreError> {
    let read = RowReader {
        table: "scim_users",
        id: &row.id,
    };
    Ok(ScimUser {
        id: read.uuid(&row.id)?,
        body: serde_json::from_str(&row.body).map_err(|_| read.corrupt("body is not JSON".to_owned()))?,
        created_at: read.timestamp(row.created_at)?,
        updated_at: read.timestamp(row.updated_at)?,
        user_name: row.user_name,
        external_id: row.external_id,
        active: row.active,
    })
}

fn group_from_row(row: scim_groups::Model) -> Result<ScimGroup, StoreError> {
    let read = RowReader {
        table: "scim_groups",
        id: &row.id,
    };
    Ok(ScimGroup {
        id: read.uuid(&row.id)?,
        body: serde_json::from_str(&row.body).map_err(|_| read.corrupt("body is not JSON".to_owned()))?,
        created_at: read.timestamp(row.created_at)?,
        updated_at: read.timestamp(row.updated_at)?,
        display_name: row.display_name,
        external_id: row.external_id,
    })
}

/// Stores a new user. Fails with [`StoreError::Duplicate`] when the user name is taken.
pub async fn insert_user(db: &impl ConnectionTrait, user: &ScimUser) -> Result<(), StoreError> {
    let row = scim_users::ActiveModel {
        id: Set(id_to_db(&user.id)),
        user_name: Set(user.user_name.clone()),
        external_id: Set(user.external_id.clone()),
        active: Set(user.active),
        body: Set(body_to_string("scim_users", user.id, &user.body)?),
        created_at: Set(user.created_at.as_millis()),
        updated_at: Set(user.updated_at.as_millis()),
    };
    scim_users::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// Writes everything of the user but its ID and creation time. Fails with [`StoreError::Duplicate`] when the user
/// name is taken.
pub async fn update_user(db: &impl ConnectionTrait, user: &ScimUser) -> Result<(), StoreError> {
    let row = scim_users::ActiveModel {
        id: Set(id_to_db(&user.id)),
        user_name: Set(user.user_name.clone()),
        external_id: Set(user.external_id.clone()),
        active: Set(user.active),
        body: Set(body_to_string("scim_users", user.id, &user.body)?),
        updated_at: Set(user.updated_at.as_millis()),
        ..Default::default()
    };
    scim_users::Entity::update(row).exec(db).await?;
    Ok(())
}

/// The user, or `None`.
pub async fn find_user(db: &impl ConnectionTrait, id: Uuid) -> Result<Option<ScimUser>, StoreError> {
    scim_users::Entity::find_by_id(id_to_db(&id))
        .one(db)
        .await?
        .map(user_from_row)
        .transpose()
}

/// Reads the user and takes a lock on its row until the transaction ends (`FOR UPDATE`), so two writes of one user run
/// one after the other, and the read sees what the earlier one committed.
pub async fn lock_user(db: &impl ConnectionTrait, id: Uuid) -> Result<Option<ScimUser>, StoreError> {
    scim_users::Entity::find_by_id(id_to_db(&id))
        .lock_exclusive()
        .one(db)
        .await?
        .map(user_from_row)
        .transpose()
}

/// Takes a lock on every user whose external ID is `external_id` until the transaction ends (`FOR UPDATE`), so a write
/// to one of them ([`lock_user`]) and a first sign-in of the person with that subject run one after the other, and the
/// later one's reads see what the earlier one committed.
pub async fn lock_users_by_external_id(db: &impl ConnectionTrait, external_id: &str) -> Result<(), StoreError> {
    scim_users::Entity::find()
        .filter(scim_users::Column::ExternalId.eq(external_id))
        .lock_exclusive()
        .all(db)
        .await?;
    Ok(())
}

/// Every user whose external ID is `external_id`, oldest first.
pub async fn list_users_by_external_id(
    db: &impl ConnectionTrait,
    external_id: &str,
) -> Result<Vec<ScimUser>, StoreError> {
    scim_users::Entity::find()
        .filter(scim_users::Column::ExternalId.eq(external_id))
        .order_by_asc(scim_users::Column::CreatedAt)
        .order_by_asc(scim_users::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(user_from_row)
        .collect()
}

/// Every external ID some user has.
pub async fn list_external_ids(db: &impl ConnectionTrait) -> Result<BTreeSet<String>, StoreError> {
    Ok(scim_users::Entity::find()
        .select_only()
        .column(scim_users::Column::ExternalId)
        .filter(scim_users::Column::ExternalId.is_not_null())
        .distinct()
        .into_tuple::<String>()
        .all(db)
        .await?
        .into_iter()
        .collect())
}

/// Users in creation order, those whose `field` equals `value` when a filter is given, from `offset`, at most `limit`.
pub async fn list_users(
    db: &impl ConnectionTrait,
    filter: Option<(UserField, &str)>,
    offset: u64,
    limit: u64,
) -> Result<Page<ScimUser>, StoreError> {
    let mut query = scim_users::Entity::find();
    if let Some((field, value)) = filter {
        query = query.filter(match field {
            UserField::UserName => scim_users::Column::UserName.eq(value),
            UserField::ExternalId => scim_users::Column::ExternalId.eq(value),
        });
    }
    let total = query.clone().count(db).await?;
    let rows = query
        .order_by_asc(scim_users::Column::CreatedAt)
        .order_by_asc(scim_users::Column::Id)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(user_from_row)
        .collect::<Result<_, _>>()?;
    Ok(Page { total, rows })
}

/// Deletes the user. The caller has removed its memberships.
pub async fn delete_user(db: &impl ConnectionTrait, id: Uuid) -> Result<(), StoreError> {
    scim_users::Entity::delete_by_id(id_to_db(&id)).exec(db).await?;
    Ok(())
}

/// The IDs among `ids` that name a stored user.
pub async fn existing_user_ids(db: &impl ConnectionTrait, ids: &BTreeSet<Uuid>) -> Result<BTreeSet<Uuid>, StoreError> {
    let ids: Vec<String> = ids.iter().map(id_to_db).collect();
    let mut found = BTreeSet::new();
    for chunk in ids.chunks(IN_LIST_CHUNK) {
        let rows = scim_users::Entity::find()
            .filter(scim_users::Column::Id.is_in(chunk.to_vec()))
            .all(db)
            .await?;
        for row in rows {
            let read = RowReader {
                table: "scim_users",
                id: &row.id,
            };
            found.insert(read.uuid(&row.id)?);
        }
    }
    Ok(found)
}

/// Stores a new group. Fails with [`StoreError::Duplicate`] when the display name is taken.
pub async fn insert_group(db: &impl ConnectionTrait, group: &ScimGroup) -> Result<(), StoreError> {
    let row = scim_groups::ActiveModel {
        id: Set(id_to_db(&group.id)),
        display_name: Set(group.display_name.clone()),
        external_id: Set(group.external_id.clone()),
        body: Set(body_to_string("scim_groups", group.id, &group.body)?),
        created_at: Set(group.created_at.as_millis()),
        updated_at: Set(group.updated_at.as_millis()),
    };
    scim_groups::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// Writes everything of the group but its ID and creation time. Fails with [`StoreError::Duplicate`] when the
/// display name is taken.
pub async fn update_group(db: &impl ConnectionTrait, group: &ScimGroup) -> Result<(), StoreError> {
    let row = scim_groups::ActiveModel {
        id: Set(id_to_db(&group.id)),
        display_name: Set(group.display_name.clone()),
        external_id: Set(group.external_id.clone()),
        body: Set(body_to_string("scim_groups", group.id, &group.body)?),
        updated_at: Set(group.updated_at.as_millis()),
        ..Default::default()
    };
    scim_groups::Entity::update(row).exec(db).await?;
    Ok(())
}

/// The group, or `None`.
pub async fn find_group(db: &impl ConnectionTrait, id: Uuid) -> Result<Option<ScimGroup>, StoreError> {
    scim_groups::Entity::find_by_id(id_to_db(&id))
        .one(db)
        .await?
        .map(group_from_row)
        .transpose()
}

/// Reads the group and takes a lock on its row until the transaction ends (`FOR UPDATE`), as [`lock_user`] does.
pub async fn lock_group(db: &impl ConnectionTrait, id: Uuid) -> Result<Option<ScimGroup>, StoreError> {
    scim_groups::Entity::find_by_id(id_to_db(&id))
        .lock_exclusive()
        .one(db)
        .await?
        .map(group_from_row)
        .transpose()
}

/// Whether a stored group has one of `names` as its display name or its external ID.
pub async fn any_group_named(db: &impl ConnectionTrait, names: &BTreeSet<String>) -> Result<bool, StoreError> {
    let names: Vec<&String> = names.iter().collect();
    for chunk in names.chunks(IN_LIST_CHUNK) {
        let found = scim_groups::Entity::find()
            .filter(
                Condition::any()
                    .add(scim_groups::Column::DisplayName.is_in(chunk.to_vec()))
                    .add(scim_groups::Column::ExternalId.is_in(chunk.to_vec())),
            )
            .limit(1)
            .all(db)
            .await?;
        if !found.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Groups in creation order, those whose `field` equals `value` when a filter is given, from `offset`, at most `limit`.
pub async fn list_groups(
    db: &impl ConnectionTrait,
    filter: Option<(GroupField, &str)>,
    offset: u64,
    limit: u64,
) -> Result<Page<ScimGroup>, StoreError> {
    let mut query = scim_groups::Entity::find();
    if let Some((field, value)) = filter {
        query = query.filter(match field {
            GroupField::DisplayName => scim_groups::Column::DisplayName.eq(value),
            GroupField::ExternalId => scim_groups::Column::ExternalId.eq(value),
        });
    }
    let total = query.clone().count(db).await?;
    let rows = query
        .order_by_asc(scim_groups::Column::CreatedAt)
        .order_by_asc(scim_groups::Column::Id)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(group_from_row)
        .collect::<Result<_, _>>()?;
    Ok(Page { total, rows })
}

/// Deletes the group. The caller has removed its members.
pub async fn delete_group(db: &impl ConnectionTrait, id: Uuid) -> Result<(), StoreError> {
    scim_groups::Entity::delete_by_id(id_to_db(&id)).exec(db).await?;
    Ok(())
}

/// The users in the group.
pub async fn list_group_members(db: &impl ConnectionTrait, group_id: Uuid) -> Result<BTreeSet<Uuid>, StoreError> {
    let rows = scim_group_members::Entity::find()
        .filter(scim_group_members::Column::GroupId.eq(id_to_db(&group_id)))
        .all(db)
        .await?;
    rows.into_iter()
        .map(|row| {
            RowReader {
                table: "scim_group_members",
                id: &row.group_id,
            }
            .uuid(&row.user_id)
        })
        .collect()
}

/// Makes `members` the group's members.
pub async fn replace_group_members(
    db: &impl ConnectionTrait,
    group_id: Uuid,
    members: &BTreeSet<Uuid>,
) -> Result<(), StoreError> {
    delete_group_members(db, group_id).await?;
    let rows: Vec<_> = members
        .iter()
        .map(|user_id| scim_group_members::ActiveModel {
            group_id: Set(id_to_db(&group_id)),
            user_id: Set(id_to_db(user_id)),
        })
        .collect();
    for chunk in rows.chunks(IN_LIST_CHUNK) {
        scim_group_members::Entity::insert_many(chunk.to_vec())
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

/// Removes every member of the group.
pub async fn delete_group_members(db: &impl ConnectionTrait, group_id: Uuid) -> Result<(), StoreError> {
    scim_group_members::Entity::delete_many()
        .filter(scim_group_members::Column::GroupId.eq(id_to_db(&group_id)))
        .exec(db)
        .await?;
    Ok(())
}

/// Removes the user from every group.
pub async fn delete_user_memberships(db: &impl ConnectionTrait, user_id: Uuid) -> Result<(), StoreError> {
    scim_group_members::Entity::delete_many()
        .filter(scim_group_members::Column::UserId.eq(id_to_db(&user_id)))
        .exec(db)
        .await?;
    Ok(())
}

/// The display name and external ID of every group the user is in.
pub async fn list_user_group_names(
    db: &impl ConnectionTrait,
    user_id: Uuid,
) -> Result<Vec<(String, Option<String>)>, StoreError> {
    let group_ids: Vec<String> = scim_group_members::Entity::find()
        .filter(scim_group_members::Column::UserId.eq(id_to_db(&user_id)))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.group_id)
        .collect();
    let mut names = Vec::new();
    for chunk in group_ids.chunks(IN_LIST_CHUNK) {
        let rows = scim_groups::Entity::find()
            .filter(scim_groups::Column::Id.is_in(chunk.to_vec()))
            .all(db)
            .await?;
        names.extend(rows.into_iter().map(|row| (row.display_name, row.external_id)));
    }
    Ok(names)
}
