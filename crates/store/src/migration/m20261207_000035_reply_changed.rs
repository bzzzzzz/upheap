//! Whether a question's reply replaced an earlier one (`item_questions.reply_changed`).
//!
//! `false` for every question stored before this migration, which were never changed, so no row needs a backfill.
//! The store writes it with the reply: a question without a reply has it `false`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemQuestions {
    Table,
    ReplyChanged,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ItemQuestions::Table)
                    .add_column(
                        ColumnDef::new(ItemQuestions::ReplyChanged)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }
}
