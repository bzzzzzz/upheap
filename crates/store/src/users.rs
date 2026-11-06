//! Users and their local password credentials.

use domain::user::{DisplayName, Email, User, UserQuery};
use domain::{Timestamp, UserId, Version};
use sea_orm::ExprTrait;
use sea_orm::sea_query::{Expr, Func, LikeExpr};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{password_credentials, users};

const TABLE: &str = "users";

/// Inserts `user`. Fails with [`StoreError::Duplicate`] when the email is taken.
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
    row.map(to_user).transpose()
}

pub async fn find_user_by_email(db: &impl ConnectionTrait, email: &Email) -> Result<Option<User>, StoreError> {
    let row = users::Entity::find()
        .filter(users::Column::Email.eq(email.as_str()))
        .one(db)
        .await?;
    row.map(to_user).transpose()
}

/// The users among `ids`, in no particular order. Unknown IDs are left out.
pub async fn list_users(db: &impl ConnectionTrait, ids: &[UserId]) -> Result<Vec<User>, StoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    users::Entity::find()
        .filter(users::Column::Id.is_in(ids.iter().map(|id| id_to_db(id.as_uuid()))))
        .all(db)
        .await?
        .into_iter()
        .map(to_user)
        .collect()
}

/// Up to `limit` users with a word of their display name starting with `query`, ignoring ASCII case, by display name.
///
/// `LOWER` lowercases ASCII only on SQLite, which is why [`UserQuery`] lowercases ASCII only too.
pub async fn find_users_by_name(
    db: &impl ConnectionTrait,
    query: &UserQuery,
    limit: u64,
) -> Result<Vec<User>, StoreError> {
    let text = escape_like(query.as_str());
    let name = || Expr::expr(Func::lower(Expr::col(users::Column::DisplayName)));
    let like = |pattern: String| LikeExpr::new(pattern).escape('\\');
    users::Entity::find()
        .filter(
            Condition::any()
                .add(name().like(like(format!("{text}%"))))
                .add(name().like(like(format!("% {text}%")))),
        )
        .order_by_asc(users::Column::DisplayName)
        .order_by_asc(users::Column::Id)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(to_user)
        .collect()
}

/// Up to `limit` users whose email starts with `prefix`, by email. Emails are stored lowercased.
pub async fn find_users_by_email_prefix(
    db: &impl ConnectionTrait,
    prefix: &str,
    limit: u64,
) -> Result<Vec<User>, StoreError> {
    let pattern = format!("{}%", escape_like(prefix));
    users::Entity::find()
        .filter(Expr::col(users::Column::Email).like(LikeExpr::new(pattern).escape('\\')))
        .order_by_asc(users::Column::Email)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(to_user)
        .collect()
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

fn to_user(row: users::Model) -> Result<User, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    Ok(User {
        id: UserId::from_uuid(read.uuid(&row.id)?),
        email: Email::parse(&row.email).map_err(|e| read.corrupt(e.to_string()))?,
        display_name: DisplayName::parse(&row.display_name).map_err(|e| read.corrupt(e.to_string()))?,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}
