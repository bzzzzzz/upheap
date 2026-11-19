//! Email change links. Only a hash of each token is stored, as for password reset links (architecture.md,
//! Authentication and authorization).
//!
//! Using and cancelling are conditional updates, which every backend applies to the latest committed row whatever
//! the transaction read before: of two requests racing to use one link, exactly one marks it used.

use domain::email_change::EmailChangeLink;
use domain::user::Email;
use domain::{Timestamp, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::email_change_links;

/// Stores a new link. Fails with [`StoreError::Duplicate`] on a token collision, which 256 random bits make a bug
/// rather than bad luck.
pub async fn insert_email_change_link(
    db: &impl ConnectionTrait,
    token_hash: &str,
    link: &EmailChangeLink,
) -> Result<(), StoreError> {
    let row = email_change_links::ActiveModel {
        token_hash: Set(token_hash.to_owned()),
        user_id: Set(id_to_db(link.user_id.as_uuid())),
        new_email: Set(link.new_email.as_str().to_owned()),
        created_at: Set(link.created_at.as_millis()),
        expires_at: Set(link.expires_at.as_millis()),
        used_at: Set(link.used_at.map(Timestamp::as_millis)),
        cancelled_at: Set(link.cancelled_at.map(Timestamp::as_millis)),
    };
    email_change_links::Entity::insert(row)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// The link with this token hash, or `None`.
pub async fn find_email_change_link(
    db: &impl ConnectionTrait,
    token_hash: &str,
) -> Result<Option<EmailChangeLink>, StoreError> {
    email_change_links::Entity::find_by_id(token_hash.to_owned())
        .one(db)
        .await?
        .map(to_email_change_link)
        .transpose()
}

/// Cancels every link of `user_id` that is still working at `now`: not used, not cancelled and not ended. Returns how
/// many it cancelled.
pub async fn cancel_working_email_change_links(
    db: &impl ConnectionTrait,
    user_id: UserId,
    now: Timestamp,
) -> Result<u64, StoreError> {
    let result = email_change_links::Entity::update_many()
        .col_expr(email_change_links::Column::CancelledAt, now.as_millis().into())
        .filter(email_change_links::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(email_change_links::Column::UsedAt.is_null())
        .filter(email_change_links::Column::CancelledAt.is_null())
        .filter(email_change_links::Column::ExpiresAt.gt(now.as_millis()))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Marks the link used at `now`, if it is still working at `now`. Returns whether it did: `false` means another
/// request used it first, or it was cancelled or ended since it was read.
pub async fn use_email_change_link(
    db: &impl ConnectionTrait,
    token_hash: &str,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let result = email_change_links::Entity::update_many()
        .col_expr(email_change_links::Column::UsedAt, now.as_millis().into())
        .filter(email_change_links::Column::TokenHash.eq(token_hash))
        .filter(email_change_links::Column::UsedAt.is_null())
        .filter(email_change_links::Column::CancelledAt.is_null())
        .filter(email_change_links::Column::ExpiresAt.gt(now.as_millis()))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

fn to_email_change_link(row: email_change_links::Model) -> Result<EmailChangeLink, StoreError> {
    // The token hash is derived from a secret, so the row is identified by its user instead.
    let read = RowReader {
        table: "email_change_links",
        id: &row.user_id,
    };
    Ok(EmailChangeLink {
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        new_email: Email::parse(&row.new_email).map_err(|e| read.corrupt(e.to_string()))?,
        created_at: read.timestamp(row.created_at)?,
        expires_at: read.timestamp(row.expires_at)?,
        used_at: read.optional_timestamp(row.used_at)?,
        cancelled_at: read.optional_timestamp(row.cancelled_at)?,
    })
}
