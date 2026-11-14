//! What a run reached (`item_runs.reached`): the stage code, or `became`, set when the run ends as reached.
//!
//! `NULL` for every run that did not end as reached, and for every run that ended before this migration, so no row
//! needs a backfill.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::string_len_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemRuns {
    Table,
    Reached,
}

const REACHED_LEN: u32 = 16;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ItemRuns::Table)
                    .add_column(string_len_null(ItemRuns::Reached, REACHED_LEN))
                    .to_owned(),
            )
            .await
    }
}
