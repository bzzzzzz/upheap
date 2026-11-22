//! Identities at an OpenID Connect provider: which user is which subject there (architecture.md, Authentication and
//! authorization). The provider is the issuer string as configured, and the subject is the value of the configured
//! subject claim of the ID token.

use domain::{Timestamp, UserId};
use sea_orm::sea_query::{Expr, Query};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::{user_identities, users};

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

/// Re-keys the identity `from_subject` at `provider` to `to_subject`. The row count is ignored: when a concurrent
/// sign-in of the same person moved it first, this is a no-op.
pub async fn move_identity(
    db: &impl ConnectionTrait,
    provider: &str,
    from_subject: &str,
    to_subject: &str,
) -> Result<(), StoreError> {
    user_identities::Entity::update_many()
        .col_expr(user_identities::Column::Subject, Expr::value(to_subject))
        .filter(user_identities::Column::Provider.eq(provider))
        .filter(user_identities::Column::Subject.eq(from_subject))
        .exec(db)
        .await?;
    Ok(())
}

/// Whether `user_id` has an identity at a provider.
pub async fn has_identity(db: &impl ConnectionTrait, user_id: UserId) -> Result<bool, StoreError> {
    let row = user_identities::Entity::find()
        .filter(user_identities::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .limit(1)
        .one(db)
        .await?;
    Ok(row.is_some())
}

/// Every user with no identity at a provider: the local accounts.
pub async fn list_users_without_identity(db: &impl ConnectionTrait) -> Result<Vec<UserId>, StoreError> {
    let with_identity = Query::select()
        .column(user_identities::Column::UserId)
        .from(user_identities::Entity)
        .to_owned();
    let ids = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(users::Column::Id.not_in_subquery(with_identity))
        .into_tuple::<String>()
        .all(db)
        .await?;
    ids.into_iter()
        .map(|id| {
            let read = RowReader {
                table: "users",
                id: &id,
            };
            read.uuid(&id).map(UserId::from_uuid)
        })
        .collect()
}

/// The subject `user_id` has at `provider`, or `None` when they have no identity there.
pub async fn find_identity_subject(
    db: &impl ConnectionTrait,
    provider: &str,
    user_id: UserId,
) -> Result<Option<String>, StoreError> {
    let row = user_identities::Entity::find()
        .filter(user_identities::Column::Provider.eq(provider))
        .filter(user_identities::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .one(db)
        .await?;
    Ok(row.map(|row| row.subject))
}

/// The users with an identity at `provider`, each with their subject there.
pub async fn list_identity_subjects(
    db: &impl ConnectionTrait,
    provider: &str,
) -> Result<std::collections::HashMap<UserId, String>, StoreError> {
    let rows = user_identities::Entity::find()
        .filter(user_identities::Column::Provider.eq(provider))
        .all(db)
        .await?;
    rows.into_iter()
        .map(|row| {
            let read = RowReader {
                table: "user_identities",
                id: &row.subject,
            };
            Ok((UserId::from_uuid(read.uuid(&row.user_id)?), row.subject))
        })
        .collect()
}
