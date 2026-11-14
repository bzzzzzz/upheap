//! A person's priority for a heap: where items of several heaps are ordered together, their scores are multiplied by
//! it. One row per person and heap, written the first time the person sets one; a heap with no row has priority 1.
//!
//! The table only holds the person's own setting, so a heap's other members never read it. Rows are kept when the
//! person leaves the team (rejoining restores the priority) and are never deleted.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, char_len, small_integer};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum HeapPriorities {
    Table,
    UserId,
    HeapId,
    Priority,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HeapPriorities::Table)
                    .col(char_len(HeapPriorities::UserId, ID_LEN))
                    .col(char_len(HeapPriorities::HeapId, ID_LEN))
                    .col(small_integer(HeapPriorities::Priority))
                    .col(big_integer(HeapPriorities::UpdatedAt))
                    .primary_key(Index::create().col(HeapPriorities::UserId).col(HeapPriorities::HeapId))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_heap_priorities_user")
                            .from(HeapPriorities::Table, HeapPriorities::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_heap_priorities_heap")
                            .from(HeapPriorities::Table, HeapPriorities::HeapId)
                            .to(Heaps::Table, Heaps::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await
    }
}
