//! Users and their local password credentials.

use domain::user::{DisplayName, Email, User};
use domain::{Timestamp, UserId, Version};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{password_credentials, users};

const TABLE: &str = "users";

/// Inserts `user`. Fails with [`StoreError::Duplicate`] when the email is taken.
pub async fn insert_user(db: &impl ConnectionTrait, user: &User) -> Result<(), StoreError> {
    let id = id_to_db(user.id.as_uuid());
    let row = users::ActiveModel {
        version: Set(version_to_db(Version::INITIAL, TABLE, &id)?),
        id: Set(id),
        email: Set(user.email.as_str().to_owned()),
        display_name: Set(user.display_name.as_str().to_owned()),
        created_at: Set(user.created_at.as_millis()),
    };
    users::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

pub async fn find_user(db: &impl ConnectionTrait, id: UserId) -> Result<Option<User>, StoreError> {
    let row = users::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_user).transpose()
}

pub async fn find_user_by_email(db: &impl ConnectionTrait, email: &Email) -> Result<Option<User>, StoreError> {
    let row = users::Entity::find()
        .filter(users::Column::Email.eq(email.as_str()))
        .one(db)
        .await?;
    row.map(to_user).transpose()
}

/// Stores the argon2 PHC string for `user_id`.
pub async fn insert_password(
    db: &impl ConnectionTrait,
    user_id: UserId,
    password_hash: &str,
    now: Timestamp,
) -> Result<(), StoreError> {
    let row = password_credentials::ActiveModel {
        user_id: Set(id_to_db(user_id.as_uuid())),
        password_hash: Set(password_hash.to_owned()),
        updated_at: Set(now.as_millis()),
    };
    password_credentials::Entity::insert(row)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// The argon2 PHC string for `user_id`, or `None` for users without a local password.
pub async fn find_password_hash(db: &impl ConnectionTrait, user_id: UserId) -> Result<Option<String>, StoreError> {
    let row = password_credentials::Entity::find_by_id(id_to_db(user_id.as_uuid()))
        .one(db)
        .await?;
    Ok(row.map(|r| r.password_hash))
}

fn to_user(row: users::Model) -> Result<User, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    Ok(User {
        id: UserId::from_uuid(read.uuid(&row.id)?),
        email: Email::parse(&row.email).map_err(|e| read.corrupt(e.to_string()))?,
        display_name: DisplayName::parse(&row.display_name).map_err(|e| read.corrupt(e.to_string()))?,
        created_at: read.timestamp(row.created_at)?,
    })
}
