//! The send a turn belongs to (`item_turns.send_id`): the ID of the send's first turn, so the next stage of an
//! all-the-way send and a split's turn for its task can be told from turns the person opened.
//!
//! Every turn stored before this migration is its own send: `send_id` is filled with its `id`. The column is added
//! nullable, filled, then made `NOT NULL` on MySQL and Postgres. SQLite can't change a column without rebuilding its
//! table, which its foreign keys make unsafe inside a migration, so there it stays nullable and the store always
//! writes it, as with the handles migration (`m20261112_000009_handles`). The index serves the queue's question of
//! which sends have had a run.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemTurns {
    Table,
    SendId,
}

fn send_id() -> ColumnDef {
    ColumnDef::new(ItemTurns::SendId).char_len(ID_LEN).to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ItemTurns::Table)
                    .add_column(send_id().null().to_owned())
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared("UPDATE item_turns SET send_id = id")
            .await?;
        if manager.get_database_backend() != DbBackend::Sqlite {
            manager
                .alter_table(
                    Table::alter()
                        .table(ItemTurns::Table)
                        .modify_column(send_id().not_null().to_owned())
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("ix_item_turns_send")
                    .table(ItemTurns::Table)
                    .col(ItemTurns::SendId)
                    .to_owned(),
            )
            .await
    }
}
