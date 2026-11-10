//! A heap's context (`heaps.context`): plain text saying what the project is and where its work lives. Nullable, so
//! the heaps that exist already have none.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::text_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Context,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Heaps::Table)
                    .add_column(text_null(Heaps::Context))
                    .to_owned(),
            )
            .await
    }
}
