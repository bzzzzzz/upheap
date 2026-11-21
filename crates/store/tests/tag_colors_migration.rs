//! The tag colours migration on a database that already has tags: every tag gets a colour from the palette, and tags
//! that share a heap don't share one while a colour is unused.

use anyhow::{Context, Result};
use domain::TagId;
use domain::tag::TagColor;
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, Statement};
use uuid::Uuid;

/// Migrations before the tag colours one.
const BEFORE_TAG_COLORS: u32 = 51;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

async fn color_of(db: &DatabaseConnection, n: u128) -> Result<TagColor> {
    let tag = store::tags::find_tag(db, TagId::from_uuid(Uuid::from_u128(n)))
        .await?
        .with_context(|| format!("tag {n}"))?;
    Ok(tag.color)
}

#[gtest]
#[tokio::test]
async fn existing_tags_each_get_a_colour_and_tags_of_one_heap_get_different_ones() -> Result<()> {
    // Arrange: team 1 with its own tag 5, and heaps 2 and 3 with tags 6 and 7 in heap 2 and tag 8 in heap 3.
    let db = store::testing::database_before_migration(BEFORE_TAG_COLORS).await?;
    run(
        &db,
        &format!(
            "INSERT INTO teams (id, slug, name, created_at, version) VALUES ('{}', 'infra', 'Infra', 0, 1)",
            id(1)
        ),
    )
    .await?;
    for heap in [2, 3] {
        run(
            &db,
            &format!(
                "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, created_at, version) \
                 VALUES ('{}', '{}', 'heap{heap}', 'Heap', 10, 10, 10, 0, 1)",
                id(heap),
                id(1)
            ),
        )
        .await?;
    }
    run(
        &db,
        &format!(
            "INSERT INTO tags (id, team_id, name, name_key, created_at, version) VALUES ('{}', '{}', 'Ops', 'ops', 0, 1)",
            id(5),
            id(1)
        ),
    )
    .await?;
    for (tag, heap, name) in [(6, 2, "a"), (7, 2, "b"), (8, 3, "c")] {
        run(
            &db,
            &format!(
                "INSERT INTO tags (id, heap_id, name, name_key, created_at, version) VALUES ('{}', '{}', '{name}', '{name}', {tag}, 1)",
                id(tag),
                id(heap)
            ),
        )
        .await?;
    }

    // Act
    store::run_migrations(&db).await?;

    // Assert: every tag reads back with a colour, the team tag and the two heap tags of heap 2 all differ.
    let team_tag = color_of(&db, 5).await?;
    let first = color_of(&db, 6).await?;
    let second = color_of(&db, 7).await?;
    let other_heap = color_of(&db, 8).await?;
    expect_that!(first, not(eq(team_tag)));
    expect_that!(second, not(any![eq(team_tag), eq(first)]));
    expect_that!(other_heap, not(eq(team_tag)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_merged_heap_tag_still_gets_a_colour_but_doesnt_use_one_up() -> Result<()> {
    // Arrange: team 1 with its own tag 5, heap 2 with tag 6 merged into tag 5 and then tag 7.
    let db = store::testing::database_before_migration(BEFORE_TAG_COLORS).await?;
    run(
        &db,
        &format!(
            "INSERT INTO teams (id, slug, name, created_at, version) VALUES ('{}', 'infra', 'Infra', 0, 1)",
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, created_at, version) \
             VALUES ('{}', '{}', 'heap2', 'Heap', 10, 10, 10, 0, 1)",
            id(2),
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO tags (id, team_id, name, name_key, created_at, version) VALUES ('{}', '{}', 'Ops', 'ops', 0, 1)",
            id(5),
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO tags (id, heap_id, name, name_key, merged_into, created_at, version) \
             VALUES ('{}', '{}', 'a', 'a', '{}', 6, 1)",
            id(6),
            id(2),
            id(5)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO tags (id, heap_id, name, name_key, created_at, version) VALUES ('{}', '{}', 'b', 'b', 7, 1)",
            id(7),
            id(2)
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert: the team tag took blue; the merged tag took sky without counting it, so the next tag takes sky too.
    expect_that!(color_of(&db, 5).await?, eq(TagColor::Blue));
    // A merged tag is hidden from `find_tag`, so its colour is read from the row.
    let merged = db
        .query_one_raw(Statement::from_string(
            db.get_database_backend(),
            format!("SELECT color FROM tags WHERE id = '{}'", id(6)),
        ))
        .await?
        .context("merged tag row")?;
    expect_that!(merged.try_get_by_index::<String>(0)?, eq("sky"));
    expect_that!(color_of(&db, 7).await?, eq(TagColor::Sky));
    Ok(())
}
