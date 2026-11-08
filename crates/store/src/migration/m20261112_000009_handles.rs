//! Readable handles (design/readable-ids.md): team and heap slugs, item keys, and the personal team that holds each
//! user's username as its slug.
//!
//! The columns are added nullable, filled, then indexed. Filling them:
//!
//! - Each user's personal team is the team made with them at registration: the one created at the user's
//!   `created_at` with them as its only member. A user who left that team, or shared it, gets a new empty team
//!   instead, and the old one stays a shared team. New teams take the user's `created_at`, since a migration has no
//!   clock it may read, and get no audit entry, since no one made them.
//! - Usernames are derived from display names, then team slugs from the other teams' names, then heap slugs from
//!   heap names within each team, oldest first, with `-2`, `-3`… where one is taken.
//! - Items get random keys.
//!
//! MySQL and Postgres then make the columns `NOT NULL`. SQLite can't change a column without rebuilding its table,
//! which its foreign keys make unsafe inside a migration, so there they stay nullable and the store always writes
//! them.
//!
//! This migration was edited after release, against the rule (design/engineering-principles.md, Data 4): it made
//! the columns `NOT NULL` on MySQL only, and now does on every backend but SQLite, so that Postgres gets them too.
//! What it does on SQLite and MySQL is unchanged, and no Postgres database had run it before.

use std::collections::{BTreeSet, HashMap};

use domain::handle::{Handle, ItemKey};
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend};
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    DisplayName,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
    Slug,
    Name,
    PersonalUserId,
    CreatedAt,
    Version,
}

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    TeamId,
    UserId,
    Role,
    JoinedAt,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
    TeamId,
    Slug,
    Name,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
    HeapId,
    Key,
}

/// What a fallback handle is made from when a name gives none, as "日本" doesn't.
const FALLBACK_USER: &str = "user";
const FALLBACK_TEAM: &str = "team";
const FALLBACK_HEAP: &str = "heap";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_columns(manager).await?;
        let db = manager.get_connection();
        fill_teams(db).await?;
        fill_heaps(db).await?;
        fill_items(db).await?;
        if manager.get_database_backend() != DbBackend::Sqlite {
            require_columns(manager).await?;
        }
        add_indexes(manager).await
    }
}

async fn add_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let add = |table: TableRef, column: ColumnDef| Table::alter().table(table).add_column(column).to_owned();
    manager
        .alter_table(add(
            Teams::Table.into_table_ref(),
            slug_column(Teams::Slug).null().to_owned(),
        ))
        .await?;
    manager
        .alter_table(add(
            Heaps::Table.into_table_ref(),
            slug_column(Heaps::Slug).null().to_owned(),
        ))
        .await?;
    manager
        .alter_table(add(Items::Table.into_table_ref(), key_column().null().to_owned()))
        .await?;
    // SQLite takes a foreign key only on a column added with it (see the blockers migration).
    if manager.get_database_backend() == DbBackend::Sqlite {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE teams ADD COLUMN personal_user_id char(36) NULL \
                 REFERENCES users (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
            )
            .await?;
        return Ok(());
    }
    manager
        .alter_table(
            Table::alter()
                .table(Teams::Table)
                .add_column(ColumnDef::new(Teams::PersonalUserId).char_len(36).null())
                .add_foreign_key(
                    TableForeignKey::new()
                        .name("fk_teams_personal_user")
                        .from_tbl(Teams::Table)
                        .from_col(Teams::PersonalUserId)
                        .to_tbl(Users::Table)
                        .to_col(Users::Id)
                        .on_delete(ForeignKeyAction::Restrict)
                        .on_update(ForeignKeyAction::Restrict),
                )
                .to_owned(),
        )
        .await
}

fn slug_column(column: impl IntoIden) -> ColumnDef {
    let max = <u32 as std::convert::TryFrom<usize>>::try_from(Handle::MAX_CHARS).unwrap_or(u32::MAX);
    ColumnDef::new(column).string_len(max).to_owned()
}

fn key_column() -> ColumnDef {
    let len = <u32 as std::convert::TryFrom<usize>>::try_from(ItemKey::LEN).unwrap_or(u32::MAX);
    ColumnDef::new(Items::Key).char_len(len).to_owned()
}

/// Handles taken so far in one namespace.
#[derive(Default)]
struct Taken(BTreeSet<String>);

impl Taken {
    /// A free handle made from `name`, or from `fallback` when the name gives none, and marks it taken.
    fn claim(&mut self, name: &str, fallback: &str) -> Result<String, DbErr> {
        let base = Handle::derive(name)
            .or_else(|| Handle::derive(fallback))
            .ok_or_else(|| DbErr::Migration(format!("no handle can be made from {fallback:?}")))?;
        #[allow(clippy::expect_used)]
        let free = std::iter::once(base.clone())
            .chain((2..).map(|n| base.with_suffix(n)))
            .find(|handle| !self.0.contains(handle.as_str()))
            .map(|handle| handle.as_str().to_owned())
            .expect("the suffixes run on until one is free, as only finitely many handles are taken");
        self.0.insert(free.clone());
        Ok(free)
    }
}

struct TeamRow {
    id: String,
    name: String,
    created_at: i64,
}

async fn fill_teams(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    let users = db
        .query_all(
            &Query::select()
                .columns([Users::Id, Users::DisplayName, Users::CreatedAt])
                .from(Users::Table)
                .order_by(Users::CreatedAt, Order::Asc)
                .order_by(Users::Id, Order::Asc)
                .to_owned(),
        )
        .await?;
    let teams = db
        .query_all(
            &Query::select()
                .columns([Teams::Id, Teams::Name, Teams::CreatedAt])
                .from(Teams::Table)
                .order_by(Teams::CreatedAt, Order::Asc)
                .order_by(Teams::Id, Order::Asc)
                .to_owned(),
        )
        .await?
        .into_iter()
        .map(|row| {
            Ok(TeamRow {
                id: row.try_get("", "id")?,
                name: row.try_get("", "name")?,
                created_at: row.try_get("", "created_at")?,
            })
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    let mut members: HashMap<String, Vec<String>> = HashMap::new();
    for row in db
        .query_all(
            &Query::select()
                .columns([TeamMembers::TeamId, TeamMembers::UserId])
                .from(TeamMembers::Table)
                .to_owned(),
        )
        .await?
    {
        let team_id: String = row.try_get("", "team_id")?;
        members.entry(team_id).or_default().push(row.try_get("", "user_id")?);
    }

    let mut taken = Taken::default();
    let mut personal: BTreeSet<String> = BTreeSet::new();
    for row in &users {
        let user_id: String = row.try_get("", "id")?;
        let display_name: String = row.try_get("", "display_name")?;
        let created_at: i64 = row.try_get("", "created_at")?;
        let username = taken.claim(&display_name, FALLBACK_USER)?;
        let own = teams.iter().find(|team| {
            team.created_at == created_at
                && !personal.contains(&team.id)
                && members.get(&team.id).is_some_and(|m| m.as_slice() == [user_id.clone()])
        });
        let team_id = match own {
            Some(team) => team.id.clone(),
            None => insert_personal_team(db, &user_id, &display_name, created_at).await?,
        };
        set_team_handles(db, &team_id, &username, Some(&user_id)).await?;
        personal.insert(team_id);
    }
    for team in teams.iter().filter(|team| !personal.contains(&team.id)) {
        let slug = taken.claim(&team.name, FALLBACK_TEAM)?;
        set_team_handles(db, &team.id, &slug, None).await?;
    }
    tracing::info!(
        users = users.len(),
        teams = teams.len(),
        "usernames and team slugs filled"
    );
    Ok(())
}

/// A new team for a user whose own team was shared or left, with them as its admin. Returns its ID.
async fn insert_personal_team(
    db: &impl ConnectionTrait,
    user_id: &str,
    display_name: &str,
    created_at: i64,
) -> Result<String, DbErr> {
    let millis = <u64 as std::convert::TryFrom<i64>>::try_from(created_at)
        .map_err(|_| DbErr::Migration(format!("user {user_id} was created before 1970 ({created_at})")))?;
    let nanos = <u32 as std::convert::TryFrom<u64>>::try_from((millis % 1000) * 1_000_000)
        .map_err(|_| DbErr::Migration("under a second of nanoseconds fits in u32".to_owned()))?;
    let ts = uuid::Timestamp::from_unix(uuid::NoContext, millis / 1000, nanos);
    let team_id = Uuid::new_v7(ts).hyphenated().to_string();
    db.execute(
        &Query::insert()
            .into_table(Teams::Table)
            .columns([Teams::Id, Teams::Name, Teams::CreatedAt, Teams::Version])
            .values_panic([team_id.clone().into(), display_name.into(), created_at.into(), 1.into()])
            .to_owned(),
    )
    .await?;
    db.execute(
        &Query::insert()
            .into_table(TeamMembers::Table)
            .columns([
                TeamMembers::TeamId,
                TeamMembers::UserId,
                TeamMembers::Role,
                TeamMembers::JoinedAt,
            ])
            .values_panic([
                team_id.clone().into(),
                user_id.into(),
                "admin".into(),
                created_at.into(),
            ])
            .to_owned(),
    )
    .await?;
    tracing::info!(
        user_id,
        team_id,
        "personal team made for a user whose own team was shared or left"
    );
    Ok(team_id)
}

async fn set_team_handles(
    db: &impl ConnectionTrait,
    team_id: &str,
    slug: &str,
    personal_user_id: Option<&str>,
) -> Result<(), DbErr> {
    db.execute(
        &Query::update()
            .table(Teams::Table)
            .value(Teams::Slug, slug)
            .value(Teams::PersonalUserId, personal_user_id.map(str::to_owned))
            .and_where(Expr::col(Teams::Id).eq(team_id))
            .to_owned(),
    )
    .await?;
    Ok(())
}

async fn fill_heaps(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    let heaps = db
        .query_all(
            &Query::select()
                .columns([Heaps::Id, Heaps::TeamId, Heaps::Name])
                .from(Heaps::Table)
                .order_by(Heaps::CreatedAt, Order::Asc)
                .order_by(Heaps::Id, Order::Asc)
                .to_owned(),
        )
        .await?;
    let mut taken: HashMap<String, Taken> = HashMap::new();
    for row in &heaps {
        let id: String = row.try_get("", "id")?;
        let team_id: String = row.try_get("", "team_id")?;
        let name: String = row.try_get("", "name")?;
        let slug = taken.entry(team_id).or_default().claim(&name, FALLBACK_HEAP)?;
        db.execute(
            &Query::update()
                .table(Heaps::Table)
                .value(Heaps::Slug, slug)
                .and_where(Expr::col(Heaps::Id).eq(id))
                .to_owned(),
        )
        .await?;
    }
    tracing::info!(heaps = heaps.len(), "heap slugs filled");
    Ok(())
}

async fn fill_items(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    let items = db
        .query_all(
            &Query::select()
                .columns([Items::Id, Items::HeapId])
                .from(Items::Table)
                .order_by(Items::Id, Order::Asc)
                .to_owned(),
        )
        .await?;
    let mut taken: HashMap<String, BTreeSet<ItemKey>> = HashMap::new();
    for row in &items {
        let id: String = row.try_get("", "id")?;
        let heap_id: String = row.try_get("", "heap_id")?;
        let keys = taken.entry(heap_id).or_default();
        let key = loop {
            let mut random = [0u8; ItemKey::LEN];
            getrandom::fill(&mut random).map_err(|e| DbErr::Migration(format!("no random source: {e}")))?;
            if let Some(key) = ItemKey::from_random(random).filter(|key| !keys.contains(key)) {
                break key;
            }
        };
        keys.insert(key);
        db.execute(
            &Query::update()
                .table(Items::Table)
                .value(Items::Key, key.as_str())
                .and_where(Expr::col(Items::Id).eq(id))
                .to_owned(),
        )
        .await?;
    }
    tracing::info!(items = items.len(), "item keys filled");
    Ok(())
}

async fn require_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let modify = |table: TableRef, column: ColumnDef| Table::alter().table(table).modify_column(column).to_owned();
    manager
        .alter_table(modify(
            Teams::Table.into_table_ref(),
            slug_column(Teams::Slug).not_null().to_owned(),
        ))
        .await?;
    manager
        .alter_table(modify(
            Heaps::Table.into_table_ref(),
            slug_column(Heaps::Slug).not_null().to_owned(),
        ))
        .await?;
    manager
        .alter_table(modify(
            Items::Table.into_table_ref(),
            key_column().not_null().to_owned(),
        ))
        .await
}

async fn add_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let unique = |name: &str, table: TableRef| Index::create().name(name).table(table).unique().to_owned();
    manager
        .create_index(
            unique("idx_teams_slug", Teams::Table.into_table_ref())
                .col(Teams::Slug)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            unique("idx_teams_personal_user", Teams::Table.into_table_ref())
                .col(Teams::PersonalUserId)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            unique("idx_heaps_team_slug", Heaps::Table.into_table_ref())
                .col(Heaps::TeamId)
                .col(Heaps::Slug)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            unique("idx_items_heap_key", Items::Table.into_table_ref())
                .col(Items::HeapId)
                .col(Items::Key)
                .to_owned(),
        )
        .await
}
