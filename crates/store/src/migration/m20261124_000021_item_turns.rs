//! Turns: an item handed to an agent (`item_turns`), the heap's setting that agents prepare new notes
//! (`heaps.agents_prepare`), and withdrawn questions (`item_questions.withdrawn_at` and `withdrawn_by`).
//!
//! A turn is its own row, so sending an item or taking it back never bumps the item's `version`. An item has at
//! most one open turn (`ended_at` is `NULL`); the service keeps it so under the heap lock, so no constraint
//! enforces it. `end_kind` says how an ended turn ended, and is named so because `end` is a reserved word on
//! Postgres. `line` may be 2,000 characters, up to 8,000 bytes, which fits MySQL's `TEXT`, but follows the
//! long-text pattern of `m20261122_000019_long_text_on_mysql` for consistency. Rows are never deleted.
//!
//! `heaps.agents_prepare` is off for the heaps that exist; the default fills existing rows, and the store always
//! writes the column. A question is withdrawn exactly when `withdrawn_at` is set; its `state` column says
//! `withdrawn` then, so the queries for open questions need no new test. Closing an item now withdraws its open
//! questions, so the ones left open on items already closed are withdrawn here, at the item's `closed_at` and by
//! no one, and reopening such an item doesn't bring them back.
//!
//! As with `items.closed_by`, SQLite gets the foreign key on the added column itself, since it can't add a
//! constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, char_len_null, string_len, text_null};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemTurns {
    Table,
    Id,
    ItemId,
    Reach,
    Goal,
    Line,
    OpenedBy,
    Source,
    OpenedAt,
    EndedAt,
    EndedBy,
    EndKind,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    AgentsPrepare,
}

#[derive(DeriveIden)]
enum ItemQuestions {
    Table,
    WithdrawnAt,
    WithdrawnBy,
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

fn user_key(name: &'static str, column: ItemTurns) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(ItemTurns::Table, column)
        .to(Users::Table, Users::Id)
        .on_delete(ForeignKeyAction::Restrict)
        .on_update(ForeignKeyAction::Restrict)
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ItemTurns::Table)
                    .col(char_len(ItemTurns::Id, ID_LEN).primary_key())
                    .col(char_len(ItemTurns::ItemId, ID_LEN))
                    .col(string_len(ItemTurns::Reach, 16))
                    .col(string_len(ItemTurns::Goal, 16))
                    .col(text_null(ItemTurns::Line))
                    .col(char_len_null(ItemTurns::OpenedBy, ID_LEN))
                    .col(string_len(ItemTurns::Source, 16))
                    .col(big_integer(ItemTurns::OpenedAt))
                    .col(big_integer_null(ItemTurns::EndedAt))
                    .col(char_len_null(ItemTurns::EndedBy, ID_LEN))
                    .col(ColumnDef::new(ItemTurns::EndKind).string_len(16).null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_turns_item")
                            .from(ItemTurns::Table, ItemTurns::ItemId)
                            .to(Items::Table, Items::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(&mut user_key("fk_item_turns_opened_by", ItemTurns::OpenedBy))
                    .foreign_key(&mut user_key("fk_item_turns_ended_by", ItemTurns::EndedBy))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_item_turns_item_ended")
                    .table(ItemTurns::Table)
                    .col(ItemTurns::ItemId)
                    .col(ItemTurns::EndedAt)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::MySql {
            manager
                .get_connection()
                .execute_unprepared("ALTER TABLE item_turns MODIFY line MEDIUMTEXT NULL")
                .await?;
        }

        // One column per statement: SQLite adds only one per `ALTER TABLE`.
        manager
            .alter_table(
                Table::alter()
                    .table(Heaps::Table)
                    .add_column(ColumnDef::new(Heaps::AgentsPrepare).boolean().not_null().default(false))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemQuestions::Table)
                    .add_column(ColumnDef::new(ItemQuestions::WithdrawnAt).big_integer().null())
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE item_questions ADD COLUMN withdrawn_by char(36) NULL \
                     REFERENCES users (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(ItemQuestions::Table)
                        .add_column(ColumnDef::new(ItemQuestions::WithdrawnBy).char_len(ID_LEN).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_item_questions_withdrawn_by")
                                .from_tbl(ItemQuestions::Table)
                                .from_col(ItemQuestions::WithdrawnBy)
                                .to_tbl(Users::Table)
                                .to_col(Users::Id)
                                .on_delete(ForeignKeyAction::Restrict)
                                .on_update(ForeignKeyAction::Restrict),
                        )
                        .to_owned(),
                )
                .await?;
        }
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE item_questions \
                 SET state = 'withdrawn', \
                     withdrawn_at = (SELECT items.closed_at FROM items WHERE items.id = item_questions.item_id) \
                 WHERE state = 'open' AND item_id IN \
                     (SELECT id FROM items WHERE state IN ('done', 'wont_do') AND closed_at IS NOT NULL)",
            )
            .await?;
        Ok(())
    }
}
