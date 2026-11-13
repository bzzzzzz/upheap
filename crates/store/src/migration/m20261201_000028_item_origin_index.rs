//! An index on `items.origin_id`, for the lookup of what was made from an item: a note's Became, the check that
//! something was made before it becomes, and the Closed list.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    OriginId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
