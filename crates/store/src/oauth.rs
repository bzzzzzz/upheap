//! Authorization codes, token families and tokens for non-web clients. Only hashes of codes and tokens are stored,
//! as for sessions.
//!
//! Single use is enforced by conditional updates (`used_at IS NULL`), which every backend applies to the latest
//! committed row whatever the transaction read before. Of two requests racing to use one code or refresh token,
//! exactly one marks it used. Revocation is per family: a token is good only while its family isn't revoked.

use std::collections::HashMap;

use domain::oauth::{CodeChallenge, DeviceName, TokenKind};
use domain::{Timestamp, TokenFamilyId, UserId};
use sea_orm::sea_query::Query;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::{oauth_auth_codes, oauth_token_families, oauth_tokens};

/// Errors name a code, family or token by its user or family: hashes are derived from secrets, which don't belong in
/// logs.
const CODES: &str = "oauth_auth_codes";
const FAMILIES: &str = "oauth_token_families";
const TOKENS: &str = "oauth_tokens";

/// An authorization code to store, as approved for `user_id`.
#[derive(Debug)]
pub struct NewAuthCode<'a> {
    pub code_hash: &'a str,
    pub user_id: UserId,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a CodeChallenge,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

/// A stored authorization code.
#[derive(Debug)]
pub struct AuthCode {
    pub user_id: UserId,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: CodeChallenge,
    pub expires_at: Timestamp,
    /// When it was exchanged, and the token family that exchange issued. `None` while it's unused.
    pub used: Option<(Timestamp, TokenFamilyId)>,
}

/// One sign-in by a client: the tokens a code was exchanged for and every pair refreshed from them.
#[derive(Debug)]
pub struct TokenFamily {
    pub id: TokenFamilyId,
    pub user_id: UserId,
    pub client_id: String,
    /// When the family was revoked, which ends every token in it. `None` while it's live.
    pub revoked_at: Option<Timestamp>,
}

/// A sign-in that can still be used: its family isn't revoked and it holds a refresh token that is unused and
/// unexpired.
#[derive(Debug)]
pub struct LiveTokenFamily {
    pub id: TokenFamilyId,
    pub client_id: String,
    /// What the client called the machine, when it did.
    pub device_name: Option<String>,
    pub created_at: Timestamp,
    /// When the family's newest token was issued: the sign-in, or its latest refresh.
    pub last_used_at: Timestamp,
}

/// A token to store.
#[derive(Debug)]
pub struct NewToken<'a> {
    pub token_hash: &'a str,
    pub family_id: TokenFamilyId,
    pub kind: TokenKind,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

/// A stored token, with its family.
#[derive(Debug)]
pub struct Token {
    pub family: TokenFamily,
    pub kind: TokenKind,
    pub expires_at: Timestamp,
    /// When a refresh token was exchanged for the next pair. Always `None` for access tokens.
    pub used_at: Option<Timestamp>,
}

pub async fn insert_token_family(
    db: &impl ConnectionTrait,
    id: TokenFamilyId,
    user_id: UserId,
    client_id: &str,
    device_name: Option<&DeviceName>,
    created_at: Timestamp,
) -> Result<(), StoreError> {
    let row = oauth_token_families::ActiveModel {
        id: Set(id_to_db(id.as_uuid())),
        user_id: Set(id_to_db(user_id.as_uuid())),
        client_id: Set(client_id.to_owned()),
        created_at: Set(created_at.as_millis()),
        revoked_at: Set(None),
        device_name: Set(device_name.map(|name| name.as_str().to_owned())),
    };
    oauth_token_families::Entity::insert(row)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// Reads the family under a row lock until the transaction ends, so a refresh and a revocation of one family run one
/// after the other. MySQL and Postgres take `FOR UPDATE`, which waits for a revocation in progress and then reads
/// the row as it committed. SQLite has no row locks and one writer at a time: a transaction that read before another
/// committed fails on its first write instead.
pub async fn lock_token_family(
    db: &impl ConnectionTrait,
    id: TokenFamilyId,
) -> Result<Option<TokenFamily>, StoreError> {
    let row = oauth_token_families::Entity::find_by_id(id_to_db(id.as_uuid()))
        .lock_exclusive()
        .one(db)
        .await?;
    row.map(to_family).transpose()
}

/// The family, revoked or not.
pub async fn find_token_family(
    db: &impl ConnectionTrait,
    id: TokenFamilyId,
) -> Result<Option<TokenFamily>, StoreError> {
    let row = oauth_token_families::Entity::find_by_id(id_to_db(id.as_uuid()))
        .one(db)
        .await?;
    row.map(to_family).transpose()
}

/// The user's live sign-ins at `now`, most recently used first, then by id. Two queries however many there are: the
/// families, then the newest token of each. A user has a handful (one per machine signed in), so neither is paged. The scans rely on
/// `idx_oauth_token_families_user` and `idx_oauth_tokens_family` (migration 14); the foreign keys have no index of
/// their own.
pub async fn list_live_token_families(
    db: &impl ConnectionTrait,
    user_id: UserId,
    now: Timestamp,
) -> Result<Vec<LiveTokenFamily>, StoreError> {
    let live_refresh_tokens = Query::select()
        .column(oauth_tokens::Column::FamilyId)
        .from(oauth_tokens::Entity)
        .and_where(oauth_tokens::Column::Kind.eq(kind_to_db(TokenKind::Refresh)))
        .and_where(oauth_tokens::Column::UsedAt.is_null())
        .and_where(oauth_tokens::Column::ExpiresAt.gt(now.as_millis()))
        .to_owned();
    let families = oauth_token_families::Entity::find()
        .filter(oauth_token_families::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(oauth_token_families::Column::RevokedAt.is_null())
        .filter(oauth_token_families::Column::Id.in_subquery(live_refresh_tokens))
        .order_by_asc(oauth_token_families::Column::Id)
        .all(db)
        .await?;
    let ids: Vec<&str> = families.iter().map(|family| family.id.as_str()).collect();
    let newest: HashMap<String, i64> = oauth_tokens::Entity::find()
        .select_only()
        .column(oauth_tokens::Column::FamilyId)
        .column_as(oauth_tokens::Column::CreatedAt.max(), "last_used_at")
        .filter(oauth_tokens::Column::FamilyId.is_in(ids))
        .group_by(oauth_tokens::Column::FamilyId)
        .into_tuple::<(String, i64)>()
        .all(db)
        .await?
        .into_iter()
        .collect();
    let mut live = Vec::with_capacity(families.len());
    for family in families {
        let read = RowReader {
            table: FAMILIES,
            id: &family.id,
        };
        let created_at = read.timestamp(family.created_at)?;
        // A family with a live refresh token has tokens, and the newest is at least as late as the family.
        let last_used_at = newest
            .get(&family.id)
            .map_or(Ok(created_at), |millis| read.timestamp(*millis))?;
        live.push(LiveTokenFamily {
            id: TokenFamilyId::from_uuid(read.uuid(&family.id)?),
            client_id: family.client_id,
            device_name: family.device_name,
            created_at,
            last_used_at,
        });
    }
    // Stable, so the order by id above settles ties.
    live.sort_by_key(|family| std::cmp::Reverse(family.last_used_at));
    Ok(live)
}

/// Revokes the family at `now`, which ends every token in it, and returns whether this call did (`false` when it
/// was revoked already). One row, so it waits for a refresh of the family in progress, and that refresh's new pair
/// is revoked with the rest.
pub async fn revoke_token_family(
    db: &impl ConnectionTrait,
    family_id: TokenFamilyId,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let result = oauth_token_families::Entity::update_many()
        .col_expr(oauth_token_families::Column::RevokedAt, now.as_millis().into())
        .filter(oauth_token_families::Column::Id.eq(id_to_db(family_id.as_uuid())))
        .filter(oauth_token_families::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

pub async fn insert_token(db: &impl ConnectionTrait, token: &NewToken<'_>) -> Result<(), StoreError> {
    let row = oauth_tokens::ActiveModel {
        token_hash: Set(token.token_hash.to_owned()),
        family_id: Set(id_to_db(token.family_id.as_uuid())),
        kind: Set(kind_to_db(token.kind).to_owned()),
        created_at: Set(token.created_at.as_millis()),
        expires_at: Set(token.expires_at.as_millis()),
        used_at: Set(None),
    };
    oauth_tokens::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The token with this hash and its family, of either kind, whatever their state.
pub async fn find_token(db: &impl ConnectionTrait, token_hash: &str) -> Result<Option<Token>, StoreError> {
    let Some(row) = oauth_tokens::Entity::find_by_id(token_hash.to_owned()).one(db).await? else {
        return Ok(None);
    };
    let read = RowReader {
        table: TOKENS,
        id: &row.family_id,
    };
    let family = oauth_token_families::Entity::find_by_id(row.family_id.clone())
        .one(db)
        .await?
        .ok_or_else(|| read.corrupt("its family is missing".to_owned()))?;
    let kind = kind_from_db(&row.kind).ok_or_else(|| read.corrupt(format!("unknown kind {:?}", row.kind)))?;
    Ok(Some(Token {
        family: to_family(family)?,
        kind,
        expires_at: read.timestamp(row.expires_at)?,
        used_at: read.optional_timestamp(row.used_at)?,
    }))
}

/// The user of an access token that hasn't expired at `now`, in a family that isn't revoked.
pub async fn find_access_token_user(
    db: &impl ConnectionTrait,
    token_hash: &str,
    now: Timestamp,
) -> Result<Option<UserId>, StoreError> {
    let row = oauth_tokens::Entity::find_by_id(token_hash.to_owned())
        .filter(oauth_tokens::Column::Kind.eq(kind_to_db(TokenKind::Access)))
        .filter(oauth_tokens::Column::ExpiresAt.gt(now.as_millis()))
        .one(db)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let family = oauth_token_families::Entity::find_by_id(row.family_id)
        .filter(oauth_token_families::Column::RevokedAt.is_null())
        .one(db)
        .await?;
    family.map(|family| Ok(to_family(family)?.user_id)).transpose()
}

/// Marks a refresh token used at `now`. `false` when it was used already, so the caller mustn't issue the next pair
/// for it.
pub async fn use_refresh_token(
    db: &impl ConnectionTrait,
    token_hash: &str,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let result = oauth_tokens::Entity::update_many()
        .col_expr(oauth_tokens::Column::UsedAt, now.as_millis().into())
        .filter(oauth_tokens::Column::TokenHash.eq(token_hash))
        .filter(oauth_tokens::Column::Kind.eq(kind_to_db(TokenKind::Refresh)))
        .filter(oauth_tokens::Column::UsedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

fn to_family(row: oauth_token_families::Model) -> Result<TokenFamily, StoreError> {
    let read = RowReader {
        table: FAMILIES,
        id: &row.id,
    };
    Ok(TokenFamily {
        id: TokenFamilyId::from_uuid(read.uuid(&row.id)?),
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        client_id: row.client_id,
        revoked_at: read.optional_timestamp(row.revoked_at)?,
    })
}

pub async fn insert_auth_code(db: &impl ConnectionTrait, code: &NewAuthCode<'_>) -> Result<(), StoreError> {
    let row = oauth_auth_codes::ActiveModel {
        code_hash: Set(code.code_hash.to_owned()),
        user_id: Set(id_to_db(code.user_id.as_uuid())),
        client_id: Set(code.client_id.to_owned()),
        redirect_uri: Set(code.redirect_uri.to_owned()),
        code_challenge: Set(code.code_challenge.as_str().to_owned()),
        created_at: Set(code.created_at.as_millis()),
        expires_at: Set(code.expires_at.as_millis()),
        used_at: Set(None),
        family_id: Set(None),
    };
    oauth_auth_codes::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The code with this hash, used or not, expired or not.
pub async fn find_auth_code(db: &impl ConnectionTrait, code_hash: &str) -> Result<Option<AuthCode>, StoreError> {
    let Some(row) = oauth_auth_codes::Entity::find_by_id(code_hash.to_owned())
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    let read = RowReader {
        table: CODES,
        id: &row.user_id,
    };
    let used = match (row.used_at, &row.family_id) {
        (None, None) => None,
        (Some(at), Some(family)) => Some((read.timestamp(at)?, TokenFamilyId::from_uuid(read.uuid(family)?))),
        _ => return Err(read.corrupt("used_at and family_id must be set together".to_owned())),
    };
    Ok(Some(AuthCode {
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        client_id: row.client_id,
        redirect_uri: row.redirect_uri,
        code_challenge: CodeChallenge::from_stored(row.code_challenge),
        expires_at: read.timestamp(row.expires_at)?,
        used,
    }))
}

/// Marks the code used at `now`, for the tokens of `family_id`. `false` when it was used already, so the caller
/// mustn't issue tokens for it.
pub async fn use_auth_code(
    db: &impl ConnectionTrait,
    code_hash: &str,
    now: Timestamp,
    family_id: TokenFamilyId,
) -> Result<bool, StoreError> {
    let result = oauth_auth_codes::Entity::update_many()
        .col_expr(oauth_auth_codes::Column::UsedAt, now.as_millis().into())
        .col_expr(oauth_auth_codes::Column::FamilyId, id_to_db(family_id.as_uuid()).into())
        .filter(oauth_auth_codes::Column::CodeHash.eq(code_hash))
        .filter(oauth_auth_codes::Column::UsedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

const fn kind_to_db(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Access => "access",
        TokenKind::Refresh => "refresh",
    }
}

fn kind_from_db(raw: &str) -> Option<TokenKind> {
    match raw {
        "access" => Some(TokenKind::Access),
        "refresh" => Some(TokenKind::Refresh),
        _ => None,
    }
}
