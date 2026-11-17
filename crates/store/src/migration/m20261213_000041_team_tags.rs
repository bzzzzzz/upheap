//! A tag is owned by a heap or by a team. Exactly one of `tags.heap_id` and `tags.team_id` is set; the store checks
//! that when it reads a row. Existing tags keep their rows and IDs and stay heap tags.
//!
//! The team's tags need no table of their own: `item_tags`, `parameter_tags`, saved views and tag orders name tags
//! by ID with a foreign key to `tags`, so one table keeps all of them working. A team's tag names are unique among
//! the team's tags by `uq_tags_team_name_key`; the existing unique index on the heap's names stays, and `NULL`s are
//! distinct in a unique index on all three databases, so each index only constrains its own kind of tag.
//!
//! SQLite can't drop `NOT NULL` from a column, so there the table is rebuilt.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum Tags {
    Table,
    HeapId,
    TeamId,
    NameKey,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
}

/// Rebuilds `tags` the documented way (create the new table, copy, drop the old, rename, recreate the indexes).
///
/// It is one string, run by one `execute_unprepared` on one pooled connection, outside a transaction (SQLite
/// migrations aren't wrapped in one): `PRAGMA foreign_keys` is per connection and does nothing inside a
/// transaction, so the script turns it off, opens its own transaction and turns it on again. The rows are copied as
/// they are, from data whose keys were enforced, so no row can break a key.
const REBUILD_TAGS_ON_SQLITE: &str = r#"
PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE tags_new (
    id char(36) NOT NULL PRIMARY KEY,
    heap_id char(36) NULL REFERENCES heaps (id) ON DELETE RESTRICT ON UPDATE RESTRICT,
    team_id char(36) NULL REFERENCES teams (id) ON DELETE RESTRICT ON UPDATE RESTRICT,
    name varchar(50) NOT NULL,
    name_key varchar(200) NOT NULL,
    description text NULL,
    created_at integer NOT NULL,
    version integer NOT NULL
);
INSERT INTO tags_new (id, heap_id, name, name_key, description, created_at, version)
    SELECT id, heap_id, name, name_key, description, created_at, version FROM tags;
DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;
CREATE UNIQUE INDEX uq_tags_heap_name_key ON tags (heap_id, name_key);
CREATE UNIQUE INDEX uq_tags_team_name_key ON tags (team_id, name_key);
COMMIT;
PRAGMA foreign_keys = ON;
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(REBUILD_TAGS_ON_SQLITE)
                .await?;
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Tags::Table)
                    .modify_column(ColumnDef::new(Tags::HeapId).char_len(ID_LEN).null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tags::Table)
                    .add_column(ColumnDef::new(Tags::TeamId).char_len(ID_LEN).null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_tags_team")
                    .from(Tags::Table, Tags::TeamId)
                    .to(Teams::Table, Teams::Id)
                    .on_delete(ForeignKeyAction::Restrict)
                    .on_update(ForeignKeyAction::Restrict)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_tags_team_name_key")
                    .table(Tags::Table)
                    .col(Tags::TeamId)
                    .col(Tags::NameKey)
                    .unique()
                    .to_owned(),
            )
            .await
    }
}
