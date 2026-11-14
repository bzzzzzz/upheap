//! Identities at an OpenID Connect provider: which user is which subject there (architecture.md, Authentication and
//! authorization). The provider is the issuer string as configured.

use domain::{Timestamp, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::user_identities;

/// Ties `user_id` to `subject` at `provider`. Fails with [`StoreError::Duplicate`] when that subject already has a
/// user.
pub async fn insert_identity(
    db: &impl ConnectionTrait,
    user_id: UserId,
    provider: &str,
    subject: &str,
    now: Timestamp,
) -> Result<(), StoreError> {
    let row = user_identities::ActiveModel {
        user_id: Set(id_to_db(user_id.as_uuid())),
        provider: Set(provider.to_owned()),
        subject: Set(subject.to_owned()),
        created_at: Set(now.as_millis()),
    };
    user_identities::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The user who is `subject` at `provider`, or `None`.
pub async fn find_identity_user(
    db: &impl ConnectionTrait,
    provider: &str,
    subject: &str,
) -> Result<Option<UserId>, StoreError> {
    let row = user_identities::Entity::find()
        .filter(user_identities::Column::Provider.eq(provider))
        .filter(user_identities::Column::Subject.eq(subject))
        .one(db)
        .await?;
    // The subject belongs to a person, so the row is identified by its user instead.
    row.map(|row| {
        let read = RowReader {
            table: "user_identities",
            id: &row.user_id,
        };
        read.uuid(&row.user_id).map(UserId::from_uuid)
    })
    .transpose()
}
