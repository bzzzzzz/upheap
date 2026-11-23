//! A person's default heap (`users.default_heap_id`): the heap the web app opens on. `NULL` for existing rows, so
//! nobody has one. As in the comment runs migration, SQLite gets the foreign key on the added column itself, since it
//! can't add a constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Users {
    Table,
    DefaultHeapId,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

const ID_LEN: u32 = 36;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE users ADD COLUMN default_heap_id char(36) NULL \
                     REFERENCES heaps (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(Users::Table)
                        .add_column(ColumnDef::new(Users::DefaultHeapId).char_len(ID_LEN).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_users_default_heap")
                                .from_tbl(Users::Table)
                                .from_col(Users::DefaultHeapId)
                                .to_tbl(Heaps::Table)
                                .to_col(Heaps::Id)
                                .on_delete(ForeignKeyAction::Restrict)
                                .on_update(ForeignKeyAction::Restrict),
                        )
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
