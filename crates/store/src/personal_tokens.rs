//! Personal tokens (`domain::personal_token`). Only the SHA-256 of a secret is stored, as for sessions.
//!
//! Revoked and expired rows are kept, so a request that presents one can be told why it was refused; they are tiny
//! and a person makes a handful.

use domain::oauth::RevokeReason;
use domain::personal_token::LAST_USED_PRECISION;
use domain::{PersonalTokenId, Timestamp, UserId};
use sea_orm::sea_query::Condition;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, revoke_reason_to_db};
use crate::entities::personal_tokens;
use crate::oauth::TokenRevocation;

const TABLE: &str = "personal_tokens";

/// A personal token to store.
#[derive(Debug)]
pub struct NewPersonalToken<'a> {
    pub id: PersonalTokenId,
    pub user_id: UserId,
    pub name: &'a str,
    pub token_hash: &'a str,
    pub created_at: Timestamp,
    /// `None` for a token that never expires.
    pub expires_at: Option<Timestamp>,
}

/// A stored personal token, whatever its state.
#[derive(Debug)]
pub struct PersonalToken {
    pub id: PersonalTokenId,
    pub user_id: UserId,
    pub name: String,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub last_used_at: Option<Timestamp>,
    pub revocation: Option<TokenRevocation>,
}

pub async fn insert_personal_token(db: &impl ConnectionTrait, token: &NewPersonalToken<'_>) -> Result<(), StoreError> {
    let row = personal_tokens::ActiveModel {
        id: Set(id_to_db(token.id.as_uuid())),
        user_id: Set(id_to_db(token.user_id.as_uuid())),
        name: Set(token.name.to_owned()),
        token_hash: Set(token.token_hash.to_owned()),
        created_at: Set(token.created_at.as_millis()),
        expires_at: Set(token.expires_at.map(Timestamp::as_millis)),
        last_used_at: Set(None),
        revoked_at: Set(None),
        revoke_reason: Set(None),
    };
    personal_tokens::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The token with this hash, whatever its state.
pub async fn find_personal_token_by_hash(
    db: &impl ConnectionTrait,
    token_hash: &str,
) -> Result<Option<PersonalToken>, StoreError> {
    let row = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?;
    row.map(to_token).transpose()
}

/// The token, whatever its state.
pub async fn find_personal_token(
    db: &impl ConnectionTrait,
    id: PersonalTokenId,
) -> Result<Option<PersonalToken>, StoreError> {
    let row = personal_tokens::Entity::find_by_id(id_to_db(id.as_uuid()))
        .one(db)
        .await?;
    row.map(to_token).transpose()
}

/// The user's live tokens at `now`, newest first. A user has a handful, so the list isn't paged. It reads
/// `idx_personal_tokens_user`.
pub async fn list_live_personal_tokens(
    db: &impl ConnectionTrait,
    user_id: UserId,
    now: Timestamp,
) -> Result<Vec<PersonalToken>, StoreError> {
    personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(is_live(now))
        .order_by_desc(personal_tokens::Column::CreatedAt)
        // Ids sort by when they were made, so this settles ties.
        .order_by_desc(personal_tokens::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_token)
        .collect()
}

/// How many live tokens the user has at `now`.
pub async fn count_live_personal_tokens(
    db: &impl ConnectionTrait,
    user_id: UserId,
    now: Timestamp,
) -> Result<u64, StoreError> {
    let count = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(is_live(now))
        .count(db)
        .await?;
    Ok(count)
}

/// Revokes the token at `now` and returns whether this call did (`false` when it was revoked already).
pub async fn revoke_personal_token(
    db: &impl ConnectionTrait,
    id: PersonalTokenId,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let result = personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::RevokedAt, now.as_millis().into())
        .filter(personal_tokens::Column::Id.eq(id_to_db(id.as_uuid())))
        .filter(personal_tokens::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Revokes every live token of the user at `now` with `reason` and returns how many it revoked.
pub async fn revoke_user_personal_tokens(
    db: &impl ConnectionTrait,
    user_id: UserId,
    reason: Option<RevokeReason>,
    now: Timestamp,
) -> Result<u64, StoreError> {
    let result = personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::RevokedAt, now.as_millis().into())
        .col_expr(
            personal_tokens::Column::RevokeReason,
            reason.map(revoke_reason_to_db).into(),
        )
        .filter(personal_tokens::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(is_live(now))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Sets `last_used_at` to `now`, only when it is empty or a minute old or more. One conditional statement, so a
/// burst of requests writes once.
pub async fn touch_personal_token(
    db: &impl ConnectionTrait,
    id: PersonalTokenId,
    now: Timestamp,
) -> Result<(), StoreError> {
    let stale_before = now.checked_sub(LAST_USED_PRECISION).unwrap_or(Timestamp::EPOCH);
    personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::LastUsedAt, now.as_millis().into())
        .filter(personal_tokens::Column::Id.eq(id_to_db(id.as_uuid())))
        .filter(
            Condition::any()
                .add(personal_tokens::Column::LastUsedAt.is_null())
                .add(personal_tokens::Column::LastUsedAt.lte(stale_before.as_millis())),
        )
        .exec(db)
        .await?;
    Ok(())
}

/// Not revoked, and with no end or one after `now`: `domain::personal_token::TokenState::Live`.
fn is_live(now: Timestamp) -> Condition {
    Condition::all().add(personal_tokens::Column::RevokedAt.is_null()).add(
        Condition::any()
            .add(personal_tokens::Column::ExpiresAt.is_null())
            .add(personal_tokens::Column::ExpiresAt.gt(now.as_millis())),
    )
}

fn to_token(row: personal_tokens::Model) -> Result<PersonalToken, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    Ok(PersonalToken {
        id: PersonalTokenId::from_uuid(read.uuid(&row.id)?),
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        name: row.name,
        created_at: read.timestamp(row.created_at)?,
        expires_at: read.optional_timestamp(row.expires_at)?,
        last_used_at: read.optional_timestamp(row.last_used_at)?,
        revocation: read.token_revocation(row.revoked_at, row.revoke_reason.as_deref())?,
    })
}
