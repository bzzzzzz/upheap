//! Saved views: a named list view of one heap.
//!
//! `list_view` holds the view (state modes, layout, sort and filters) as JSON text, which is never queried inside
//! the database: the list is always computed in the application. Views are archived (`archived_at`), never
//! deleted. The index serves listing a heap's views.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, integer, string_len, text};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum SavedViews {
    Table,
    Id,
    HeapId,
    Name,
    ListView,
    CreatedBy,
    CreatedAt,
    ArchivedAt,
    Version,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
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

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SavedViews::Table)
                    .col(char_len(SavedViews::Id, ID_LEN).primary_key())
                    .col(char_len(SavedViews::HeapId, ID_LEN))
                    .col(string_len(SavedViews::Name, 100))
                    .col(text(SavedViews::ListView))
                    .col(char_len(SavedViews::CreatedBy, ID_LEN))
                    .col(big_integer(SavedViews::CreatedAt))
                    .col(big_integer_null(SavedViews::ArchivedAt))
                    .col(integer(SavedViews::Version))
                    .foreign_key(&mut restrict(
                        "fk_saved_views_heap",
                        (SavedViews::Table, SavedViews::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_saved_views_created_by",
                        (SavedViews::Table, SavedViews::CreatedBy),
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_saved_views_heap")
                    .table(SavedViews::Table)
                    .col(SavedViews::HeapId)
                    .to_owned(),
            )
            .await
    }
}
