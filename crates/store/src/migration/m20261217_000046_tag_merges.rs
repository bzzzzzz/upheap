//! A heap tag merged into a team tag keeps its row, with `tags.merged_into` naming the team tag. The column is a
//! plain `ADD COLUMN`; existing tags are in use, so it is `NULL` for all of them. The unique indexes stay: a merged
//! tag's `name_key` is rewritten when it is merged (see `store::tags::mark_merged`).
//!
//! SQLite can add a nullable column that references another table, so there it is one statement.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    MergedInto,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            return manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE tags ADD COLUMN merged_into char(36) NULL \
                     REFERENCES tags (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await
                .map(|_| ());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Tags::Table)
                    .add_column(ColumnDef::new(Tags::MergedInto).char_len(ID_LEN).null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_tags_merged_into")
                    .from(Tags::Table, Tags::MergedInto)
                    .to(Tags::Table, Tags::Id)
                    .on_delete(ForeignKeyAction::Restrict)
                    .on_update(ForeignKeyAction::Restrict)
                    .to_owned(),
            )
            .await
    }
}
