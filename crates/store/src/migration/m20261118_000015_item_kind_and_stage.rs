//! What each item is (`items.kind`) and how far it has been prepared (`items.stage`).
//!
//! Existing items become notes as written. The defaults are there to fill existing rows. The store always writes
//! both columns, so new rows never rely on them.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    Kind,
    Stage,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One column per statement: SQLite adds only one per `ALTER TABLE`.
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::Kind).string_len(16).not_null().default("note"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(
                        ColumnDef::new(Items::Stage)
                            .string_len(16)
                            .not_null()
                            .default("as_written"),
                    )
                    .to_owned(),
            )
            .await
    }
}
