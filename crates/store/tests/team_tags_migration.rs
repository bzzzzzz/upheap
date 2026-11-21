//! The team tags migration on a database that already has tags: each stays a heap tag with its ID, name and
//! description, items still carry it, and a team can then have tags of its own.

use anyhow::{Context, Result};
use domain::tag::{NewTag, Tag, TagColor, TagDescription, TagName, TagOwner};
use domain::{TagId, TeamId, Timestamp};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, Statement};
use uuid::Uuid;

/// Migrations before the team tags one.
const BEFORE_TEAM_TAGS: u32 = 40;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn existing_tags_stay_heap_tags_and_a_team_can_have_its_own() -> Result<()> {
    // Arrange: team 1 with heap 2, a tag 3 with a description, and item 4 carrying it.
    let db = store::testing::database_before_migration(BEFORE_TEAM_TAGS).await?;
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
             VALUES ('{}', '{}', 'backlog', 'Backlog', 10, 10, 10, 0, 1)",
            id(2),
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO tags (id, heap_id, name, name_key, description, created_at, version) \
             VALUES ('{}', '{}', 'Docs', 'docs', 'Writing', 0, 1)",
            id(3),
            id(2)
        ),
    )
    .await?;
    let backend = db.get_database_backend();
    let key = if backend == store::DbBackend::MySql {
        "`key`"
    } else {
        "\"key\""
    };
    run(
        &db,
        &format!(
            "INSERT INTO items (id, heap_id, {key}, title, state, need, created_at, version) \
             VALUES ('{}', '{}', 'k3x9a', 'Item', 'open', 'expected', 0, 1)",
            id(4),
            id(2)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO item_tags (item_id, tag_id, added_at) VALUES ('{}', '{}', 0)",
            id(4),
            id(3)
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert: the tag reads back as the heap's, and the item still carries it.
    let tag = store::tags::find_tag(&db, TagId::from_uuid(Uuid::from_u128(3)))
        .await?
        .context("tag 3")?;
    expect_that!(
        tag.owner,
        eq(TagOwner::Heap(domain::HeapId::from_uuid(Uuid::from_u128(2))))
    );
    expect_that!(tag.name.as_str(), eq("Docs"));
    expect_that!(
        tag.description.as_ref().map(TagDescription::as_str),
        some(eq("Writing"))
    );
    let row = db
        .query_one_raw(Statement::from_string(
            backend,
            format!("SELECT tag_id FROM item_tags WHERE item_id = '{}'", id(4)),
        ))
        .await?
        .context("item_tags row")?;
    expect_that!(row.try_get_by_index::<String>(0)?, eq(&id(3)));

    // And a team tag can be inserted and read, listed with the heap's tags.
    let team = TeamId::from_uuid(Uuid::from_u128(1));
    let team_tag = Tag::new(
        TagId::from_uuid(Uuid::from_u128(5)),
        TagOwner::Team(team),
        NewTag {
            name: TagName::parse("Ops")?,
            description: None,
            color: None,
        },
        TagColor::Blue,
        Timestamp::from_millis(0).context("in range")?,
    );
    store::tags::insert_tag(&db, &team_tag).await?;
    let listed = store::tags::list_tags(&db, domain::HeapId::from_uuid(Uuid::from_u128(2))).await?;
    expect_that!(listed, elements_are![eq(&tag), eq(&team_tag)]);
    expect_that!(
        store::tags::list_team_tags(&db, team).await?,
        elements_are![eq(&team_tag)]
    );
    Ok(())
}
