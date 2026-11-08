//! How much each item is needed (`items.need`), and the heap's weight for it (`heaps.need_weight`).
//!
//! Existing items become `expected`: under ranking formula `V2` that step leaves an item's age counting as it did
//! before, so items made before this migration mostly keep their order among themselves (blockers can shift; see
//! `FormulaVersion::V2`). Existing heaps get the weight every new heap starts with.
//!
//! The defaults are there to fill existing rows. The store always writes both columns, so new rows never rely on
//! them.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    Need,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    NeedWeight,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One column per statement: SQLite adds only one per `ALTER TABLE`.
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(
                        ColumnDef::new(Items::Need)
                            .string_len(16)
                            .not_null()
                            .default("expected"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Heaps::Table)
                    .add_column(ColumnDef::new(Heaps::NeedWeight).integer().not_null().default(10))
                    .to_owned(),
            )
            .await
    }
}
