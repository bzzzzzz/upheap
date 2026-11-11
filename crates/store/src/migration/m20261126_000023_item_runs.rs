//! Runs: one go an agent has at the turn of an item (`item_runs`), how many runs in a row a turn has failed
//! (`item_turns.failed_runs`), and the run a question (`item_questions.run_id`) or an audit entry
//! (`audit_log.run_id`) was made in.
//!
//! A run is its own row, so claiming one, hearing from it and ending it never bump the item's `version`. `end_kind`
//! says how an ended run ended, and is named so because `end` is a reserved word on Postgres. `result`, `links` (a
//! JSON list of up to 10 addresses of 2,000 characters) and `failure` may exceed MySQL's `TEXT`, so they follow the
//! long-text pattern of `m20261122_000019_long_text_on_mysql`. Rows are never deleted.
//!
//! The columns added to existing tables are nullable or default to 0, and no row needs a backfill: no run exists
//! yet. As in the turns migration, SQLite gets the foreign key on the added column itself, since it can't add a
//! constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, string_len};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemRuns {
    Table,
    Id,
    ItemId,
    TurnId,
    UserId,
    StartedAt,
    HeardAt,
    EndedAt,
    EndKind,
    FromStage,
    Result,
    Links,
    Failure,
    CostMicroUsd,
}

#[derive(DeriveIden)]
enum ItemTurns {
    Table,
    Id,
    FailedRuns,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum RunId {
    RunId,
}

fn reference(
    name: &'static str,
    column: ItemRuns,
    table: impl IntoIden + 'static,
    to: impl IntoIden + 'static,
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(ItemRuns::Table, column)
        .to(table, to)
        .on_delete(ForeignKeyAction::Restrict)
        .on_update(ForeignKeyAction::Restrict)
        .to_owned()
}

/// Adds the nullable column `run_id` to `table`, referring to `item_runs`.
async fn add_run_id(manager: &SchemaManager<'_>, table: &'static str, constraint: &'static str) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        manager
            .get_connection()
            .execute_unprepared(&format!(
                "ALTER TABLE {table} ADD COLUMN run_id char(36) NULL \
                 REFERENCES item_runs (id) ON DELETE RESTRICT ON UPDATE RESTRICT"
            ))
            .await?;
        return Ok(());
    }
    manager
        .alter_table(
            Table::alter()
                .table(Alias::new(table))
                .add_column(ColumnDef::new(RunId::RunId).char_len(ID_LEN).null())
                .add_foreign_key(
                    TableForeignKey::new()
                        .name(constraint)
                        .from_tbl(Alias::new(table))
                        .from_col(RunId::RunId)
                        .to_tbl(ItemRuns::Table)
                        .to_col(ItemRuns::Id)
                        .on_delete(ForeignKeyAction::Restrict)
                        .on_update(ForeignKeyAction::Restrict),
                )
                .to_owned(),
        )
        .await
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ItemRuns::Table)
                    .col(char_len(ItemRuns::Id, ID_LEN).primary_key())
                    .col(char_len(ItemRuns::ItemId, ID_LEN))
                    .col(char_len(ItemRuns::TurnId, ID_LEN))
                    .col(char_len(ItemRuns::UserId, ID_LEN))
                    .col(big_integer(ItemRuns::StartedAt))
                    .col(big_integer(ItemRuns::HeardAt))
                    .col(big_integer_null(ItemRuns::EndedAt))
                    .col(ColumnDef::new(ItemRuns::EndKind).string_len(16).null())
                    .col(string_len(ItemRuns::FromStage, 16))
                    .col(ColumnDef::new(ItemRuns::Result).text().null())
                    .col(ColumnDef::new(ItemRuns::Links).text().null())
                    .col(ColumnDef::new(ItemRuns::Failure).text().null())
                    .col(big_integer_null(ItemRuns::CostMicroUsd))
                    .foreign_key(&mut reference(
                        "fk_item_runs_item",
                        ItemRuns::ItemId,
                        Items::Table,
                        Items::Id,
                    ))
                    .foreign_key(&mut reference(
                        "fk_item_runs_turn",
                        ItemRuns::TurnId,
                        ItemTurns::Table,
                        ItemTurns::Id,
                    ))
                    .foreign_key(&mut reference(
                        "fk_item_runs_user",
                        ItemRuns::UserId,
                        Users::Table,
                        Users::Id,
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_item_runs_turn")
                    .table(ItemRuns::Table)
                    .col(ItemRuns::TurnId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_item_runs_item_ended")
                    .table(ItemRuns::Table)
                    .col(ItemRuns::ItemId)
                    .col(ItemRuns::EndedAt)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::MySql {
            for column in ["result", "links", "failure"] {
                manager
                    .get_connection()
                    .execute_unprepared(&format!("ALTER TABLE item_runs MODIFY {column} MEDIUMTEXT NULL"))
                    .await?;
            }
        }

        manager
            .alter_table(
                Table::alter()
                    .table(ItemTurns::Table)
                    .add_column(ColumnDef::new(ItemTurns::FailedRuns).integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;
        add_run_id(manager, "item_questions", "fk_item_questions_run").await?;
        add_run_id(manager, "audit_log", "fk_audit_log_run").await?;
        Ok(())
    }
}
