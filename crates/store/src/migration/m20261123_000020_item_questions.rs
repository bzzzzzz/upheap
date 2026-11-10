//! Questions and requests on an item. A question is its own row with its own `version`, so replying never bumps the
//! item's.
//!
//! `number` is given when the question is asked and never changes or repeats. `choices` and `reply_picked` are JSON
//! arrays in text columns: a question's labels and the indices picked, read and written whole. `details` and
//! `reply_comment` may each be 20,000 characters, up to 80,000 bytes in UTF-8, which MySQL's `TEXT` can't hold, so
//! they are `MEDIUMTEXT` there (see `m20261122_000019_long_text_on_mysql`). Rows are never deleted.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    big_integer, big_integer_null, boolean, char_len, char_len_null, integer, integer_null, string_len, text, text_null,
};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemQuestions {
    Table,
    Id,
    ItemId,
    Number,
    Kind,
    Text,
    Details,
    Choices,
    Many,
    StepNumber,
    State,
    AskedBy,
    AskedAt,
    ReplyPicked,
    ReplyComment,
    RepliedBy,
    RepliedAt,
    Version,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

/// Each column with its nullability, which `MODIFY` must state again.
const MYSQL_LONG_TEXT: [&str; 2] = [
    "ALTER TABLE item_questions MODIFY details MEDIUMTEXT NULL",
    "ALTER TABLE item_questions MODIFY reply_comment MEDIUMTEXT NULL",
];

fn user_key(name: &'static str, column: ItemQuestions) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(ItemQuestions::Table, column)
        .to(Users::Table, Users::Id)
        .on_delete(ForeignKeyAction::Restrict)
        .on_update(ForeignKeyAction::Restrict)
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ItemQuestions::Table)
                    .col(char_len(ItemQuestions::Id, ID_LEN).primary_key())
                    .col(char_len(ItemQuestions::ItemId, ID_LEN))
                    .col(integer(ItemQuestions::Number))
                    .col(string_len(ItemQuestions::Kind, 16))
                    .col(string_len(ItemQuestions::Text, 300))
                    .col(text_null(ItemQuestions::Details))
                    .col(text(ItemQuestions::Choices))
                    .col(boolean(ItemQuestions::Many))
                    .col(integer_null(ItemQuestions::StepNumber))
                    .col(string_len(ItemQuestions::State, 16))
                    .col(char_len_null(ItemQuestions::AskedBy, ID_LEN))
                    .col(big_integer(ItemQuestions::AskedAt))
                    .col(text_null(ItemQuestions::ReplyPicked))
                    .col(text_null(ItemQuestions::ReplyComment))
                    .col(char_len_null(ItemQuestions::RepliedBy, ID_LEN))
                    .col(big_integer_null(ItemQuestions::RepliedAt))
                    .col(integer(ItemQuestions::Version))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_questions_item")
                            .from(ItemQuestions::Table, ItemQuestions::ItemId)
                            .to(Items::Table, Items::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(&mut user_key("fk_item_questions_asked_by", ItemQuestions::AskedBy))
                    .foreign_key(&mut user_key("fk_item_questions_replied_by", ItemQuestions::RepliedBy))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_item_questions_number")
                    .table(ItemQuestions::Table)
                    .col(ItemQuestions::ItemId)
                    .col(ItemQuestions::Number)
                    .unique()
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::MySql {
            for statement in MYSQL_LONG_TEXT {
                manager.get_connection().execute_unprepared(statement).await?;
            }
        }
        Ok(())
    }
}
