//! Authorization codes and tokens for non-web clients. Only hashes of codes and tokens are stored, as for sessions.
//!
//! Single use is enforced by conditional updates (`used_at IS NULL`), which every backend applies to the latest
//! committed row whatever the transaction read before. Of two requests racing to use one code or refresh token,
//! exactly one marks it used.

use domain::oauth::{CodeChallenge, TokenKind};
use domain::{Timestamp, TokenFamilyId, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::{oauth_auth_codes, oauth_tokens};

/// Errors name a code or token by its user: the hash is derived from a secret, which doesn't belong in logs.
const CODES: &str = "oauth_auth_codes";
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

/// A token to store.
#[derive(Debug)]
pub struct NewToken<'a> {
    pub token_hash: &'a str,
    pub family_id: TokenFamilyId,
    pub kind: TokenKind,
    pub user_id: UserId,
    pub client_id: &'a str,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

/// A stored token.
#[derive(Debug)]
pub struct Token {
    pub family_id: TokenFamilyId,
    pub kind: TokenKind,
    pub user_id: UserId,
    pub client_id: String,
    pub expires_at: Timestamp,
    /// When a refresh token was exchanged for the next pair. Always `None` for access tokens.
    pub used_at: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
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

pub async fn insert_token(db: &impl ConnectionTrait, token: &NewToken<'_>) -> Result<(), StoreError> {
    let row = oauth_tokens::ActiveModel {
        token_hash: Set(token.token_hash.to_owned()),
        family_id: Set(id_to_db(token.family_id.as_uuid())),
        kind: Set(kind_to_db(token.kind).to_owned()),
        user_id: Set(id_to_db(token.user_id.as_uuid())),
        client_id: Set(token.client_id.to_owned()),
        created_at: Set(token.created_at.as_millis()),
        expires_at: Set(token.expires_at.as_millis()),
        used_at: Set(None),
        revoked_at: Set(None),
    };
    oauth_tokens::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The token with this hash, of either kind, whatever its state.
pub async fn find_token(db: &impl ConnectionTrait, token_hash: &str) -> Result<Option<Token>, StoreError> {
    let Some(row) = oauth_tokens::Entity::find_by_id(token_hash.to_owned()).one(db).await? else {
        return Ok(None);
    };
    let read = RowReader {
        table: TOKENS,
        id: &row.user_id,
    };
    Ok(Some(Token {
        family_id: TokenFamilyId::from_uuid(read.uuid(&row.family_id)?),
        kind: kind_from_db(&row.kind).ok_or_else(|| read.corrupt(format!("unknown kind {:?}", row.kind)))?,
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        client_id: row.client_id,
        expires_at: read.timestamp(row.expires_at)?,
        used_at: read.optional_timestamp(row.used_at)?,
        revoked_at: read.optional_timestamp(row.revoked_at)?,
    }))
}

/// The user of an access token that is neither revoked nor expired at `now`.
pub async fn find_access_token_user(
    db: &impl ConnectionTrait,
    token_hash: &str,
    now: Timestamp,
) -> Result<Option<UserId>, StoreError> {
    let row = oauth_tokens::Entity::find_by_id(token_hash.to_owned())
        .filter(oauth_tokens::Column::Kind.eq(kind_to_db(TokenKind::Access)))
        .filter(oauth_tokens::Column::RevokedAt.is_null())
        .filter(oauth_tokens::Column::ExpiresAt.gt(now.as_millis()))
        .one(db)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let read = RowReader {
        table: TOKENS,
        id: &row.user_id,
    };
    Ok(Some(UserId::from_uuid(read.uuid(&row.user_id)?)))
}

/// Marks a refresh token used at `now`. `false` when it was used or revoked already, so the caller mustn't issue the
/// next pair for it.
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
        .filter(oauth_tokens::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Revokes every token of the family not revoked yet, at `now`, and returns how many that was.
pub async fn revoke_token_family(
    db: &impl ConnectionTrait,
    family_id: TokenFamilyId,
    now: Timestamp,
) -> Result<u64, StoreError> {
    let result = oauth_tokens::Entity::update_many()
        .col_expr(oauth_tokens::Column::RevokedAt, now.as_millis().into())
        .filter(oauth_tokens::Column::FamilyId.eq(id_to_db(family_id.as_uuid())))
        .filter(oauth_tokens::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected)
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
