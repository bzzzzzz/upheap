//! A watch on an open turn: the pull request a run waits on, and when the watching began (`item_turns.watch_url`,
//! `watch_since`).
//!
//! Both are set together or both are `NULL`. Every turn stored before this migration has none, which reads as no
//! watch, so no row needs a backfill. `watch_since` is UTC milliseconds like the turn's other times.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer_null, text_null};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemTurns {
    Table,
    WatchUrl,
    WatchSince,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One statement per column: SQLite takes only one `ADD COLUMN` at a time.
        manager
            .alter_table(
                Table::alter()
                    .table(ItemTurns::Table)
                    .add_column(text_null(ItemTurns::WatchUrl))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemTurns::Table)
                    .add_column(big_integer_null(ItemTurns::WatchSince))
                    .to_owned(),
            )
            .await
    }
}
