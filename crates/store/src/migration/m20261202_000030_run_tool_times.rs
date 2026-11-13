//! When a run's agent tool started and exited, as the loop saw it (`item_runs.tool_started_at`, `tool_ended_at`).
//!
//! Both are `NULL` for every run stored before this migration, and for a run whose tool never started, so no row needs
//! a backfill. They are UTC milliseconds like the run's other times.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::big_integer_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemRuns {
    Table,
    ToolStartedAt,
    ToolEndedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One statement per column: SQLite takes only one `ADD COLUMN` at a time.
        manager
            .alter_table(
                Table::alter()
                    .table(ItemRuns::Table)
                    .add_column(big_integer_null(ItemRuns::ToolStartedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemRuns::Table)
                    .add_column(big_integer_null(ItemRuns::ToolEndedAt))
                    .to_owned(),
            )
            .await
    }
}
