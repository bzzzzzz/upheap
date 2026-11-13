//! Which items were split from their origin (`items.made_in_split`), and an index on `items.origin_id`.
//!
//! `origin_id` links an item to the note it was split from, and also, since runs add items, to the item of the run
//! that added it. Only the first are what the note became, so the flag tells them apart: it is set when the item is
//! made with an origin and never changed, and is false for every item stored before this. The index serves the lookup
//! of what a note became, for the item, the check before it becomes, and the closed list.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    OriginId,
    MadeInSplit,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::MadeInSplit).boolean().not_null().default(false))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_items_origin")
                    .table(Items::Table)
                    .col(Items::OriginId)
                    .to_owned(),
            )
            .await
    }
}
