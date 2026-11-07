//! An index for reading one entity's history from the audit log, newest first: the Edit item view, a tag's or a
//! parameter's history. Reading a heap's or a team's log uses the indexes from the initial migration.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    EntityId,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_entity")
                    .table(AuditLog::Table)
                    .col(AuditLog::EntityId)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await
    }
}
