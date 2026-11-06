//! Tags and the tags on each item.
//!
//! `name_key` is the lowercased name. Uniqueness is checked on it rather than left to the column collation,
//! because MySQL compares case-insensitively by default and SQLite doesn't. The key itself must then compare
//! exactly: MySQL's default collation also ignores accents ("resume" = "résumé") and folds "ß" to "ss", so the
//! column gets the binary `utf8mb4_bin` there. SQLite compares bytes already. Sorting by the key is then by code
//! point on both.
//!
//! Removing a tag from an item sets `item_tags.removed_at` instead of deleting the row; adding it again clears
//! it. Tags themselves are never deleted, so nothing is.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, integer, string_len, text_null};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    HeapId,
    Name,
    NameKey,
    Description,
    CreatedAt,
    Version,
}

#[derive(DeriveIden)]
enum ItemTags {
    Table,
    ItemId,
    TagId,
    AddedAt,
    RemovedAt,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
}

fn restrict(
    name: &str,
    from: (impl IntoIden, impl IntoIden),
    to: (impl IntoIden, impl IntoIden),
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(from.0, from.1)
        .to(to.0, to.1)
        .on_delete(ForeignKeyAction::Restrict)
        .on_update(ForeignKeyAction::Restrict)
        .to_owned()
}

/// The lowercased name, compared byte for byte on every backend.
fn name_key_column(backend: DbBackend) -> ColumnDef {
    // Lowercasing can lengthen a few characters, so the key gets room to spare.
    let mut column = string_len(Tags::NameKey, 200);
    if backend == DbBackend::MySql {
        column.extra("CHARACTER SET utf8mb4 COLLATE utf8mb4_bin");
    }
    column
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Tags::Table)
                    .col(char_len(Tags::Id, ID_LEN).primary_key())
                    .col(char_len(Tags::HeapId, ID_LEN))
                    .col(string_len(Tags::Name, 50))
                    .col(name_key_column(manager.get_database_backend()))
                    .col(text_null(Tags::Description))
                    .col(big_integer(Tags::CreatedAt))
                    .col(integer(Tags::Version))
                    .foreign_key(&mut restrict(
                        "fk_tags_heap",
                        (Tags::Table, Tags::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_tags_heap_name_key")
                    .table(Tags::Table)
                    .col(Tags::HeapId)
                    .col(Tags::NameKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ItemTags::Table)
                    .col(char_len(ItemTags::ItemId, ID_LEN))
                    .col(char_len(ItemTags::TagId, ID_LEN))
                    .col(big_integer(ItemTags::AddedAt))
                    .col(big_integer_null(ItemTags::RemovedAt))
                    .primary_key(Index::create().col(ItemTags::ItemId).col(ItemTags::TagId))
                    .foreign_key(&mut restrict(
                        "fk_item_tags_item",
                        (ItemTags::Table, ItemTags::ItemId),
                        (Items::Table, Items::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_item_tags_tag",
                        (ItemTags::Table, ItemTags::TagId),
                        (Tags::Table, Tags::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_item_tags_tag")
                    .table(ItemTags::Table)
                    .col(ItemTags::TagId)
                    .to_owned(),
            )
            .await
    }
}
