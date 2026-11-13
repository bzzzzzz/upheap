//! Reviews and redo: a review is a question with a link, a piece and a round (`item_questions.review_link`,
//! `review_piece`, `review_round`), and a turn that reached a stable state can be sent back for changes
//! (`item_turns.redone_at`, `redone_by`).
//!
//! Every added column is nullable and no row needs a backfill: an existing question reads as having no review, and an
//! existing turn as not redone. `review_link` and `review_piece` may be 2,000 characters, which MySQL's `TEXT` holds
//! in any encoding, so they stay `TEXT`. As in the runs migration, SQLite gets the foreign key on the added column
//! itself, since it can't add a constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemQuestions {
    Table,
    ReviewLink,
    ReviewPiece,
    ReviewRound,
}

#[derive(DeriveIden)]
enum ItemTurns {
    Table,
    RedoneAt,
    RedoneBy,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ItemQuestions::Table)
                    .add_column(ColumnDef::new(ItemQuestions::ReviewLink).text().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemQuestions::Table)
                    .add_column(ColumnDef::new(ItemQuestions::ReviewPiece).text().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemQuestions::Table)
                    .add_column(ColumnDef::new(ItemQuestions::ReviewRound).integer().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ItemTurns::Table)
                    .add_column(ColumnDef::new(ItemTurns::RedoneAt).big_integer().null())
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE item_turns ADD COLUMN redone_by char(36) NULL \
                     REFERENCES users (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(ItemTurns::Table)
                    .add_column(ColumnDef::new(ItemTurns::RedoneBy).char_len(ID_LEN).null())
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk_item_turns_redone_by")
                            .from_tbl(ItemTurns::Table)
                            .from_col(ItemTurns::RedoneBy)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await
    }
}
