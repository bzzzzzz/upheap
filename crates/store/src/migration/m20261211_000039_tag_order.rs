//! The heap's weight for its tag order (`heaps.tags_weight`) and the order itself (`heaps.tag_order`).
//!
//! Existing heaps get weight 0 and no order, so their ranking doesn't change. `tag_order` is a JSON list of tag IDs,
//! the highest first, and is `NULL` when the heap ranks no tag: MySQL can't give a `TEXT` column a default, so the
//! column is nullable and the store reads `NULL` as an empty order. The weight's default fills existing rows; the
//! store always writes it.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Heaps {
    Table,
    TagsWeight,
    TagOrder,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One column per statement: SQLite adds only one per `ALTER TABLE`.
        manager
            .alter_table(
                Table::alter()
                    .table(Heaps::Table)
                    .add_column(ColumnDef::new(Heaps::TagsWeight).integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Heaps::Table)
                    .add_column(ColumnDef::new(Heaps::TagOrder).text().null())
                    .to_owned(),
            )
            .await
    }
}
