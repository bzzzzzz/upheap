//! Sign-ins in flight through an OpenID Connect provider, keyed by the SHA-256 of their `state`.
//!
//! These rows are authentication plumbing, not domain data, so a sign-in's end deletes its row, and rows past their
//! end are deleted a few at a time.

use domain::Timestamp;
use domain::user::{DisplayName, Email};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};

use crate::StoreError;
use crate::codes::RowReader;
use crate::entities::oidc_sign_ins::{self, Column};

const TABLE: &str = "oidc_sign_ins";
/// Errors name no row: the key is derived from a secret.
const ROW: &str = "(oidc sign-in)";

/// A sign-in as it starts. The nonce and the PKCE verifier are secrets: never log them.
pub struct NewOidcSignIn<'a> {
    pub state_hash: &'a str,
    pub nonce: &'a str,
    pub pkce_verifier: &'a str,
    /// The app path to return to afterwards.
    pub redirect: Option<&'a str>,
    pub now: Timestamp,
    pub expires_at: Timestamp,
}

/// What the provider vouched for, kept for the username step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub subject: String,
    pub email: Email,
    pub display_name: DisplayName,
}

/// A sign-in in flight. The nonce and the PKCE verifier are secrets: never log them.
pub struct OidcSignIn {
    pub nonce: String,
    pub pkce_verifier: String,
    pub redirect: Option<String>,
    /// `None` until the callback has verified the person with the provider.
    pub verified: Option<VerifiedIdentity>,
    pub expires_at: Timestamp,
}

/// Stores a new sign-in. Fails with [`StoreError::Duplicate`] on a state collision, which 256 random bits make a bug.
pub async fn insert_oidc_sign_in(db: &impl ConnectionTrait, sign_in: &NewOidcSignIn<'_>) -> Result<(), StoreError> {
    let row = oidc_sign_ins::ActiveModel {
        state_hash: Set(sign_in.state_hash.to_owned()),
        nonce: Set(sign_in.nonce.to_owned()),
        pkce_verifier: Set(sign_in.pkce_verifier.to_owned()),
        redirect: Set(sign_in.redirect.map(str::to_owned)),
        subject: Set(None),
        email: Set(None),
        display_name: Set(None),
        created_at: Set(sign_in.now.as_millis()),
        expires_at: Set(sign_in.expires_at.as_millis()),
    };
    oidc_sign_ins::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The sign-in with this state hash, ended or not, or `None`.
pub async fn find_oidc_sign_in(db: &impl ConnectionTrait, state_hash: &str) -> Result<Option<OidcSignIn>, StoreError> {
    let Some(row) = oidc_sign_ins::Entity::find_by_id(state_hash.to_owned()).one(db).await? else {
        return Ok(None);
    };
    let read = RowReader { table: TABLE, id: ROW };
    let verified = match (row.subject, row.email, row.display_name) {
        (Some(subject), Some(email), Some(display_name)) => Some(VerifiedIdentity {
            subject,
            email: Email::parse(&email).map_err(|_| read.corrupt("invalid email".to_owned()))?,
            display_name: DisplayName::parse(&display_name)
                .map_err(|_| read.corrupt("invalid display name".to_owned()))?,
        }),
        (None, None, None) => None,
        _ => return Err(read.corrupt("verified identity is partly set".to_owned())),
    };
    Ok(Some(OidcSignIn {
        nonce: row.nonce,
        pkce_verifier: row.pkce_verifier,
        redirect: row.redirect,
        verified,
        expires_at: read.timestamp(row.expires_at)?,
    }))
}

/// Records what the provider vouched for and sets the end of the username step. Returns whether the sign-in still
/// existed.
pub async fn set_oidc_sign_in_verified(
    db: &impl ConnectionTrait,
    state_hash: &str,
    identity: &VerifiedIdentity,
    expires_at: Timestamp,
) -> Result<bool, StoreError> {
    let result = oidc_sign_ins::Entity::update_many()
        .col_expr(Column::Subject, identity.subject.as_str().into())
        .col_expr(Column::Email, identity.email.as_str().into())
        .col_expr(Column::DisplayName, identity.display_name.as_str().into())
        .col_expr(Column::ExpiresAt, expires_at.as_millis().into())
        .filter(Column::StateHash.eq(state_hash))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Deletes the sign-in. Deleting one that is gone succeeds.
pub async fn delete_oidc_sign_in(db: &impl ConnectionTrait, state_hash: &str) -> Result<(), StoreError> {
    oidc_sign_ins::Entity::delete_by_id(state_hash.to_owned())
        .exec(db)
        .await?;
    Ok(())
}

/// Deletes up to `batch` sign-ins that ended by `now`, and returns how many.
///
/// The keys are read first and then deleted, as for sign-in attempts, so the delete locks rows by key.
pub async fn delete_ended_oidc_sign_ins(
    db: &impl ConnectionTrait,
    now: Timestamp,
    batch: u64,
) -> Result<u64, StoreError> {
    let ended = Column::ExpiresAt.lte(now.as_millis());
    let keys: Vec<String> = oidc_sign_ins::Entity::find()
        .select_only()
        .column(Column::StateHash)
        .filter(ended.clone())
        .limit(batch)
        .into_tuple()
        .all(db)
        .await?;
    if keys.is_empty() {
        return Ok(0);
    }
    let result = oidc_sign_ins::Entity::delete_many()
        .filter(Column::StateHash.is_in(keys))
        .filter(ended)
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}
