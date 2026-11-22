//! The run a comment was written in (`item_comments.run_id`), so a comment an agent wrote reads as the agent's, as
//! its entry in the history does.
//!
//! Comments written before this have their run in the audit log: adding a comment writes its `comment_added` entry
//! in the same transaction, about the same item, by the same user and at the same time, and that entry has carried
//! the run since runs exist. The backfill copies it from there; a comment a person wrote, or one older than runs,
//! finds `NULL` and stays `NULL`. As in the runs migration, SQLite gets the foreign key on the added column itself,
//! since it can't add a constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemComments {
    Table,
    RunId,
}

#[derive(DeriveIden)]
enum ItemRuns {
    Table,
    Id,
}

const ID_LEN: u32 = 36;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        if manager.get_database_backend() == DbBackend::Sqlite {
            db.execute_unprepared(
                "ALTER TABLE item_comments ADD COLUMN run_id char(36) NULL \
                 REFERENCES item_runs (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
            )
            .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(ItemComments::Table)
                        .add_column(ColumnDef::new(ItemComments::RunId).char_len(ID_LEN).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_item_comments_run")
                                .from_tbl(ItemComments::Table)
                                .from_col(ItemComments::RunId)
                                .to_tbl(ItemRuns::Table)
                                .to_col(ItemRuns::Id)
                                .on_delete(ForeignKeyAction::Restrict)
                                .on_update(ForeignKeyAction::Restrict),
                        )
                        .to_owned(),
                )
                .await?;
        }
        // `MAX` settles the one case with two candidates, two comments on an item by one user in the same
        // millisecond: either both were written in the run or neither was, as one request writes one comment.
        db.execute_unprepared(
            "UPDATE item_comments SET run_id = (\
                 SELECT MAX(audit_log.run_id) FROM audit_log \
                 WHERE audit_log.action = 'comment_added' \
                   AND audit_log.entity_id = item_comments.item_id \
                   AND audit_log.actor_user_id = item_comments.created_by \
                   AND audit_log.created_at = item_comments.created_at)",
        )
        .await?;
        Ok(())
    }
}
