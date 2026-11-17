//! The agent tool that ran a run and the model it used (`item_runs.agent_tool`, `agent_model`).
//!
//! Both are `NULL` for every run stored before this migration, so no row needs a backfill.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::string_len_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemRuns {
    Table,
    AgentTool,
    AgentModel,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One statement per column: SQLite takes only one `ADD COLUMN` at a time.
        manager
            .alter_table(
                Table::alter()
                    .table(ItemRuns::Table)
                    .add_column(string_len_null(ItemRuns::AgentTool, 100))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemRuns::Table)
                    .add_column(string_len_null(ItemRuns::AgentModel, 100))
                    .to_owned(),
            )
            .await
    }
}
