//! When a reminder joins the list (`items.arrives_at`): the first millisecond of its date in the time zone of the
//! request that set it, in UTC milliseconds like the other times.
//!
//! It is its own column rather than the due's end of day minus 24 hours, since a day that changes the clocks is not 24
//! hours long. It is `NULL` for an item that was never a reminder, which includes every item stored before this
//! migration, so nothing needs a backfill. A reminder made a task keeps it, and ranking counts the task's age from
//! it. The column is nullable on every backend, and because SQLite can't make a column `NOT NULL` without rebuilding
//! its table.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    ArrivesAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::ArrivesAt).big_integer().null())
                    .to_owned(),
            )
            .await
    }
}
