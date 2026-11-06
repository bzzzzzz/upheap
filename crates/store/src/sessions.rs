//! Web sign-in sessions. Only a hash of each session token is stored.
//!
//! Sessions are authentication plumbing, not domain data, so ending one deletes its row.

use domain::{Timestamp, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::sessions;

pub async fn insert_session(
    db: &impl ConnectionTrait,
    token_hash: &str,
    user_id: UserId,
    created_at: Timestamp,
    expires_at: Timestamp,
) -> Result<(), StoreError> {
    let row = sessions::ActiveModel {
        token_hash: Set(token_hash.to_owned()),
        user_id: Set(id_to_db(user_id.as_uuid())),
        created_at: Set(created_at.as_millis()),
        expires_at: Set(expires_at.as_millis()),
    };
    sessions::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The user a session belongs to, or `None` if there is no such session or it expired before `now`.
pub async fn find_session_user(
    db: &impl ConnectionTrait,
    token_hash: &str,
    now: Timestamp,
) -> Result<Option<UserId>, StoreError> {
    let row = sessions::Entity::find_by_id(token_hash.to_owned())
        .filter(sessions::Column::ExpiresAt.gt(now.as_millis()))
        .one(db)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    // The token hash is a secret-derived value, so the row is identified by its user instead.
    let read = RowReader {
        table: "sessions",
        id: &row.user_id,
    };
    Ok(Some(UserId::from_uuid(read.uuid(&row.user_id)?)))
}

/// Ends a session. Ending a session that doesn't exist succeeds.
pub async fn delete_session(db: &impl ConnectionTrait, token_hash: &str) -> Result<(), StoreError> {
    sessions::Entity::delete_by_id(token_hash.to_owned()).exec(db).await?;
    Ok(())
}
