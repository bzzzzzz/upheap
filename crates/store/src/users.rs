//! Users and their local password credentials.
//!
//! A user's username is the slug of their personal team (`teams.personal_user_id`), so one unique index covers
//! both and no team can take a username. Every function that returns a user reads it from there.

use std::collections::HashMap;

use domain::handle::Handle;
use domain::user::{DisplayName, Email, User, UserQuery};
use domain::{Timestamp, UserId, Version};
use sea_orm::ExprTrait;
use sea_orm::sea_query::{BinOper, Expr, Func, LikeExpr};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{password_credentials, teams, users};

const TABLE: &str = "users";

/// Inserts `user`. Fails with [`StoreError::Duplicate`] when the email is taken. Their username is held by their
/// personal team, which the caller inserts next ([`crate::teams::insert_team`]).
pub async fn insert_user(db: &impl ConnectionTrait, user: &User) -> Result<(), StoreError> {
    let id = id_to_db(user.id.as_uuid());
    let row = users::ActiveModel {
        version: Set(version_to_db(user.version, TABLE, &id)?),
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
    Ok(to_users(db, row.into_iter().collect()).await?.pop())
}

/// The user whose username is `username`, or `None` if there is none.
pub async fn find_user_by_username(db: &impl ConnectionTrait, username: &Handle) -> Result<Option<User>, StoreError> {
    let team = teams::Entity::find()
        .filter(teams::Column::Slug.eq(username.as_str()))
        .filter(teams::Column::PersonalUserId.is_not_null())
        .one(db)
        .await?;
    let Some(user_id) = team.and_then(|team| team.personal_user_id) else {
        return Ok(None);
    };
    let row = users::Entity::find_by_id(user_id).one(db).await?;
    Ok(to_users(db, row.into_iter().collect()).await?.pop())
}

pub async fn find_user_by_email(db: &impl ConnectionTrait, email: &Email) -> Result<Option<User>, StoreError> {
    let row = users::Entity::find()
        .filter(users::Column::Email.eq(email.as_str()))
        .one(db)
        .await?;
    Ok(to_users(db, row.into_iter().collect()).await?.pop())
}

/// The users among `ids`, in no particular order. Unknown IDs are left out.
pub async fn list_users(db: &impl ConnectionTrait, ids: &[UserId]) -> Result<Vec<User>, StoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = users::Entity::find()
        .filter(users::Column::Id.is_in(ids.iter().map(|id| id_to_db(id.as_uuid()))))
        .all(db)
        .await?;
    to_users(db, rows).await
}

/// Up to `limit` users with a word of their display name starting with `query`, ignoring case, by display name.
///
/// Both the name and the pattern go through the database's `LOWER`, so they are lowercased alike: ASCII only on
/// SQLite, all of Unicode on Postgres and MySQL.
pub async fn find_users_by_name(
    db: &impl ConnectionTrait,
    query: &UserQuery,
    limit: u64,
) -> Result<Vec<User>, StoreError> {
    let text = escape_like(query.as_str());
    let name = || Expr::expr(Func::lower(Expr::col(users::Column::DisplayName)));
    // `LOWER(pattern) ESCAPE '\'`, built by hand because `LikeExpr` takes only a literal pattern.
    let like = |pattern: String| {
        Expr::Binary(
            Box::new(Func::lower(Expr::val(pattern)).into()),
            BinOper::Escape,
            Box::new(Expr::Constant('\\'.into())),
        )
    };
    let rows = users::Entity::find()
        .filter(
            Condition::any()
                .add(name().binary(BinOper::Like, like(format!("{text}%"))))
                .add(name().binary(BinOper::Like, like(format!("% {text}%")))),
        )
        .order_by_asc(users::Column::DisplayName)
        .order_by_asc(users::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    to_users(db, rows).await
}

/// Up to `limit` users whose email starts with `prefix`, by email. Emails are stored lowercased.
pub async fn find_users_by_email_prefix(
    db: &impl ConnectionTrait,
    prefix: &str,
    limit: u64,
) -> Result<Vec<User>, StoreError> {
    let pattern = format!("{}%", escape_like(prefix));
    let rows = users::Entity::find()
        .filter(Expr::col(users::Column::Email).like(LikeExpr::new(pattern).escape('\\')))
        .order_by_asc(users::Column::Email)
        .limit(limit)
        .all(db)
        .await?;
    to_users(db, rows).await
}

/// Writes the display name if the stored row still has version `expected`, and returns the user with its new
/// version. Returns `None` when the stored version differs.
pub async fn update_user(
    db: &impl ConnectionTrait,
    user: &User,
    expected: Version,
) -> Result<Option<User>, StoreError> {
    let id = id_to_db(user.id.as_uuid());
    let saved = User {
        version: expected.next(),
        ..user.clone()
    };
    let row = users::ActiveModel {
        display_name: Set(saved.display_name.as_str().to_owned()),
        version: Set(version_to_db(saved.version, TABLE, &id)?),
        ..Default::default()
    };
    let result = users::Entity::update_many()
        .set(row)
        .filter(users::Column::Id.eq(id.as_str()))
        .filter(users::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
}

/// Makes `%`, `_` and the escape character itself match literally in a `LIKE` pattern.
fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
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

/// Replaces the argon2 PHC string of `user_id` with `password_hash`, only while the stored one is still
/// `expected_hash`, and returns whether it did. `false` means the user has none or it changed since it was read.
pub async fn update_password(
    db: &impl ConnectionTrait,
    user_id: UserId,
    expected_hash: &str,
    password_hash: &str,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let result = password_credentials::Entity::update_many()
        .col_expr(password_credentials::Column::PasswordHash, password_hash.into())
        .col_expr(password_credentials::Column::UpdatedAt, now.as_millis().into())
        .filter(password_credentials::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .filter(password_credentials::Column::PasswordHash.eq(expected_hash))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Sets the argon2 PHC string of `user_id` to `password_hash`, whatever it was, and inserts it for a user who has
/// none. For a password reset, which doesn't know the old one ([`update_password`] does).
pub async fn set_password(
    db: &impl ConnectionTrait,
    user_id: UserId,
    password_hash: &str,
    now: Timestamp,
) -> Result<(), StoreError> {
    let result = password_credentials::Entity::update_many()
        .col_expr(password_credentials::Column::PasswordHash, password_hash.into())
        .col_expr(password_credentials::Column::UpdatedAt, now.as_millis().into())
        .filter(password_credentials::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        insert_password(db, user_id, password_hash, now).await?;
    }
    Ok(())
}

/// The users of `rows`, in the same order, with their usernames from their personal teams.
async fn to_users(db: &impl ConnectionTrait, rows: Vec<users::Model>) -> Result<Vec<User>, StoreError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let usernames: HashMap<String, String> = teams::Entity::find()
        .filter(teams::Column::PersonalUserId.is_in(rows.iter().map(|row| row.id.as_str())))
        .all(db)
        .await?
        .into_iter()
        .filter_map(|team| Some((team.personal_user_id?, team.slug)))
        .collect();
    rows.into_iter()
        .map(|row| {
            let username = usernames.get(&row.id).cloned();
            to_user(row, username)
        })
        .collect()
}

fn to_user(row: users::Model, username: Option<String>) -> Result<User, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let username = username.ok_or_else(|| read.corrupt("user has no personal team".to_owned()))?;
    Ok(User {
        id: UserId::from_uuid(read.uuid(&row.id)?),
        username: read.handle(&username)?,
        email: Email::parse(&row.email).map_err(|e| read.corrupt(e.to_string()))?,
        display_name: DisplayName::parse(&row.display_name).map_err(|e| read.corrupt(e.to_string()))?,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
