//! A parameter is owned by a heap or by a team. Exactly one of `parameters.heap_id` and `parameters.team_id` is set;
//! the store checks that when it reads a row. Existing parameters keep their rows and IDs and stay heap parameters.
//!
//! A team's parameter is one row, with its options and tag links in the tables a heap's parameters use, so every
//! table that names a parameter by ID keeps working. Each heap of the team chooses whether to use it, and weights it
//! on its own: `heap_parameters` holds that choice, one row per heap and team parameter, with the heap's weight.
//! Nothing is deleted: stopping sets `stopped_at`, and using again clears it, so the weight survives (as
//! `item_tags.removed_at` does). `used_at` records when the heap first used it, for the record; nothing reads it.
//! Only team parameters get rows.
//!
//! A team parameter's own `parameters.weight` holds `0` and is never read: the weight belongs to the heap.
//!
//! Names are unique among the parameters in use of a heap and its team, which a unique index can't express (archived
//! parameters keep their names, and the rule spans two owners), so the service checks it under the team's locks.
//!
//! SQLite can't drop `NOT NULL` from a column, so there the table is rebuilt.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, integer};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum Parameters {
    Table,
    Id,
    HeapId,
    TeamId,
}

#[derive(DeriveIden)]
enum HeapParameters {
    Table,
    HeapId,
    ParameterId,
    Weight,
    UsedAt,
    StoppedAt,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
}

/// Rebuilds `parameters` the documented way (create the new table, copy, drop the old, rename, recreate the index),
/// as the team tags migration rebuilds `tags`: one string, run on one connection outside a transaction, with
/// `PRAGMA foreign_keys` off around its own transaction. The rows are copied as they are, from data whose keys were
/// enforced, so no row can break a key.
const REBUILD_PARAMETERS_ON_SQLITE: &str = r#"
PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE parameters_new (
    id char(36) NOT NULL PRIMARY KEY,
    heap_id char(36) NULL REFERENCES heaps (id) ON DELETE RESTRICT ON UPDATE RESTRICT,
    team_id char(36) NULL REFERENCES teams (id) ON DELETE RESTRICT ON UPDATE RESTRICT,
    name varchar(50) NOT NULL,
    weight integer NOT NULL,
    created_at bigint NOT NULL,
    archived_at bigint NULL,
    version integer NOT NULL
);
INSERT INTO parameters_new (id, heap_id, name, weight, created_at, archived_at, version)
    SELECT id, heap_id, name, weight, created_at, archived_at, version FROM parameters;
DROP TABLE parameters;
ALTER TABLE parameters_new RENAME TO parameters;
CREATE INDEX idx_parameters_heap ON parameters (heap_id);
CREATE INDEX idx_parameters_team ON parameters (team_id);
COMMIT;
PRAGMA foreign_keys = ON;
"#;

fn restrict(
    name: &str,
    from: (impl IntoIden, impl IntoIden),
    to: (impl IntoIden, impl IntoIden),
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(from.0, from.1)
        .to(to.0, to.1)
        .on_delete(ForeignKeyAction::Restrict)
        .on_update(ForeignKeyAction::Restrict)
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(REBUILD_PARAMETERS_ON_SQLITE)
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(Parameters::Table)
                        .modify_column(ColumnDef::new(Parameters::HeapId).char_len(ID_LEN).null())
                        .to_owned(),
                )
                .await?;
            manager
                .alter_table(
                    Table::alter()
                        .table(Parameters::Table)
                        .add_column(ColumnDef::new(Parameters::TeamId).char_len(ID_LEN).null())
                        .to_owned(),
                )
                .await?;
            manager
                .create_foreign_key(restrict(
                    "fk_parameters_team",
                    (Parameters::Table, Parameters::TeamId),
                    (Teams::Table, Teams::Id),
                ))
                .await?;
            manager
                .create_index(
                    Index::create()
                        .name("idx_parameters_team")
                        .table(Parameters::Table)
                        .col(Parameters::TeamId)
                        .to_owned(),
                )
                .await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(HeapParameters::Table)
                    .col(char_len(HeapParameters::HeapId, ID_LEN))
                    .col(char_len(HeapParameters::ParameterId, ID_LEN))
                    .col(integer(HeapParameters::Weight))
                    .col(big_integer(HeapParameters::UsedAt))
                    .col(big_integer_null(HeapParameters::StoppedAt))
                    .primary_key(
                        Index::create()
                            .col(HeapParameters::HeapId)
                            .col(HeapParameters::ParameterId),
                    )
                    .foreign_key(&mut restrict(
                        "fk_heap_parameters_heap",
                        (HeapParameters::Table, HeapParameters::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_heap_parameters_parameter",
                        (HeapParameters::Table, HeapParameters::ParameterId),
                        (Parameters::Table, Parameters::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_heap_parameters_parameter")
                    .table(HeapParameters::Table)
                    .col(HeapParameters::ParameterId)
                    .to_owned(),
            )
            .await
    }
}
