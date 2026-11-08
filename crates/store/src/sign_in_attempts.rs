//! Sign-in attempts per email address, for rate limiting (`domain::sign_in`). Like sessions, these are
//! authentication plumbing, so rows are deleted once their window is over.

use domain::Timestamp;
use domain::sign_in::SignInAttempts;
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};

use crate::StoreError;
use crate::entities::sign_in_attempts::{self, Column};

const TABLE: &str = "sign_in_attempts";
/// Errors name no row: the key is derived from an email address, which doesn't belong in logs.
const ROW: &str = "(sign-in attempts)";

/// Counts one attempt for the address and returns its attempts so far, this one included.
///
/// An attempt at or after the end of the window opens a new one, ending at `new_window_end`. Counting is a single upsert, so concurrent
/// attempts are each counted on every backend, and none can slip past the limit by racing another.
/// Run it in a transaction to read back this attempt's own count: the upsert locks the row until the transaction
/// ends. Outside one, the count can include attempts made since, which only errs towards refusing.
pub async fn count_sign_in_attempt(
    db: &impl ConnectionTrait,
    email_hash: &str,
    now: Timestamp,
    new_window_end: Timestamp,
) -> Result<SignInAttempts, StoreError> {
    let new_end = new_window_end.as_millis();
    // Columns are named with their table: in Postgres's `ON CONFLICT` a bare name could also mean `excluded`'s.
    let current = |column: Column| Expr::col((sign_in_attempts::Entity, column));
    let ended = current(Column::WindowEndsAt).lte(now.as_millis());
    let row = sign_in_attempts::ActiveModel {
        email_hash: Set(email_hash.to_owned()),
        attempts: Set(1),
        window_ends_at: Set(new_end),
    };
    // Both expressions read the row as it was: SQLite and Postgres always do, and MySQL does here because
    // `attempts` is assigned before `window_ends_at`.
    let on_conflict = OnConflict::column(Column::EmailHash)
        .value(
            Column::Attempts,
            Expr::case(ended.clone(), 1).finally(current(Column::Attempts).add(1)),
        )
        .value(
            Column::WindowEndsAt,
            Expr::case(ended, new_end).finally(current(Column::WindowEndsAt)),
        )
        .to_owned();
    sign_in_attempts::Entity::insert(row)
        .on_conflict(on_conflict)
        .exec_without_returning(db)
        .await?;

    let row = sign_in_attempts::Entity::find_by_id(email_hash.to_owned())
        .one(db)
        .await?
        .ok_or_else(|| StoreError::corrupt(TABLE, ROW, "row missing after upsert"))?;
    let read = |problem| StoreError::corrupt(TABLE, ROW, problem);
    Ok(SignInAttempts {
        count: u32::try_from(row.attempts).map_err(|_| read("negative attempts"))?,
        window_ends_at: Timestamp::from_millis(row.window_ends_at).ok_or_else(|| read("window end out of range"))?,
    })
}

/// Forgets the address's attempts, after it signed in.
pub async fn clear_sign_in_attempts(db: &impl ConnectionTrait, email_hash: &str) -> Result<(), StoreError> {
    sign_in_attempts::Entity::delete_by_id(email_hash.to_owned())
        .exec(db)
        .await?;
    Ok(())
}

/// Deletes up to `batch` rows whose window ended by `now`, and returns how many.
///
/// The rows are found with a plain read and then deleted by key. A range delete on `window_ends_at` would lock that
/// index before the rows, the opposite order from a counting upsert, and the two could deadlock on MySQL.
pub async fn delete_ended_sign_in_windows(
    db: &impl ConnectionTrait,
    now: Timestamp,
    batch: u64,
) -> Result<u64, StoreError> {
    let ended = Column::WindowEndsAt.lte(now.as_millis());
    let keys: Vec<String> = sign_in_attempts::Entity::find()
        .select_only()
        .column(Column::EmailHash)
        .filter(ended.clone())
        .limit(batch)
        .into_tuple()
        .all(db)
        .await?;
    if keys.is_empty() {
        return Ok(0);
    }
    // The window is checked again: an attempt since the read may have opened a new one.
    let result = sign_in_attempts::Entity::delete_many()
        .filter(Column::EmailHash.is_in(keys))
        .filter(ended)
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}
