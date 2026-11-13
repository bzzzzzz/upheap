//! Why a blocked item is blocked, in words (`items.blocked_reason`).
//!
//! It is set only while the item is blocked, and only when a reason was given, so it is `NULL` for every item stored
//! before this migration and no row needs a backfill. The length matches a title's, which the domain enforces.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    BlockedReason,
}

const REASON_LEN: u32 = 300;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::BlockedReason).string_len(REASON_LEN).null())
                    .to_owned(),
            )
            .await
    }
}
