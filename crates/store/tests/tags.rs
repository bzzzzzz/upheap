//! Tag storage rules that the service's own checks would otherwise hide.
//!
//! Runs on in-memory SQLite, or on MySQL with `TEST_DATABASE_URL` set; the MySQL run in CI is what proves the
//! `utf8mb4_bin` key.

use anyhow::{Context, Result};
use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy};
use domain::tag::{NewTag, Tag, TagName};
use domain::team::Team;
use domain::text::Name;
use domain::validation::Field;
use domain::{HeapId, TagId, TeamId, Timestamp, Version};
use googletest::prelude::*;
use store::{DatabaseConnection, StoreError};
use uuid::Uuid;

async fn heap_in_new_database() -> Result<(DatabaseConnection, HeapId)> {
    let db = store::testing::fresh_database().await?;
    let now = Timestamp::from_millis(0).context("in range")?;
    let team = Team {
        id: TeamId::from_uuid(Uuid::now_v7()),
        slug: Handle::parse("team", Field::Slug)?,
        name: Name::parse("Team")?,
        personal_user: None,
        created_at: now,
        version: Version::INITIAL,
    };
    store::teams::insert_team(&db, &team).await?;
    let heap = Heap {
        id: HeapId::from_uuid(Uuid::now_v7()),
        team_id: team.id,
        slug: Handle::parse("heap", Field::Slug)?,
        name: Name::parse("Heap")?,
        policy: HeapPolicy::default(),
        context: None,
        agents_prepare: false,
        origin_id: None,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now).await?;
    Ok((db, heap.id))
}

fn tag(heap_id: HeapId, name: &str) -> Result<Tag> {
    let fields = NewTag {
        name: TagName::parse(name)?,
        description: None,
    };
    Ok(Tag::new(
        TagId::from_uuid(Uuid::now_v7()),
        heap_id,
        fields,
        Timestamp::from_millis(0).context("in range")?,
    ))
}

/// Two requests can both pass the service's name check; the unique index must stop the second one in a way the
/// service recognises, so the caller gets `tag_name_taken` rather than a 500.
#[gtest]
#[tokio::test]
async fn a_second_tag_with_the_same_key_is_a_duplicate() -> Result<()> {
    // Arrange
    let (db, heap_id) = heap_in_new_database().await?;
    store::tags::insert_tag(&db, &tag(heap_id, "Infra")?).await?;

    // Act
    let second = store::tags::insert_tag(&db, &tag(heap_id, "infra")?).await;

    // Assert
    expect_that!(second, err(matches_pattern!(StoreError::Duplicate(_))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn names_differing_only_in_accents_are_different_tags() -> Result<()> {
    // Arrange
    let (db, heap_id) = heap_in_new_database().await?;
    store::tags::insert_tag(&db, &tag(heap_id, "resume")?).await?;

    // Act
    let accented = store::tags::insert_tag(&db, &tag(heap_id, "Résumé")?).await;
    let found = store::tags::find_tag_by_name(&db, heap_id, &TagName::parse("RÉSUMÉ")?).await?;

    // Assert
    expect_that!(accented, ok(anything()));
    expect_that!(found.map(|t| t.name), some(eq(&TagName::parse("Résumé")?)));
    Ok(())
}
