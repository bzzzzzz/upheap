//! Where a person placed an item in its heap's list (`items.placement`): a positive multiplier on the item's own
//! score, which ranking applies. `NULL` means the item is not placed and ranks by its fields alone, which includes
//! every item stored before this migration, so nothing needs a backfill. The column is nullable on every backend, and
//! because SQLite can't make a column `NOT NULL` without rebuilding its table.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    Placement,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::Placement).double().null())
                    .to_owned(),
            )
            .await
    }
}
