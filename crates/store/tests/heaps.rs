//! Heap storage rules the service's own checks would otherwise hide.

use anyhow::{Context, Result};
use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy};
use domain::parameter::Weight;
use domain::team::Team;
use domain::text::{Context as HeapContext, Name};
use domain::validation::Field;
use domain::{HeapId, TagId, TeamId, Timestamp, Version};
use googletest::prelude::*;
use store::{ConnectionTrait, StoreError};
use uuid::Uuid;

#[gtest]
#[tokio::test]
async fn weights_round_trip_and_a_stored_weight_over_100_is_a_corrupt_row() -> Result<()> {
    // Arrange
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
        policy: HeapPolicy {
            need_weight: Weight::parse(100)?,
            age_weight: Weight::parse(0)?,
            tags_weight: Weight::parse(40)?,
            ..HeapPolicy::default()
        },
        tag_order: vec![TagId::from_uuid(Uuid::now_v7()), TagId::from_uuid(Uuid::now_v7())],
        context: None,
        agents_prepare: false,
        origin_id: None,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now).await?;

    // Act
    let read = store::heaps::find_heap(&db, heap.id).await?;
    db.execute_unprepared(&format!(
        "UPDATE heaps SET need_weight = 101 WHERE id = '{}'",
        heap.id.as_uuid().hyphenated()
    ))
    .await?;
    let corrupt = store::heaps::find_heap(&db, heap.id).await;

    // Assert
    expect_that!(read, some(eq(&heap)));
    expect_that!(corrupt, err(matches_pattern!(StoreError::CorruptRow { .. })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_empty_tag_order_is_stored_as_null_and_a_malformed_one_is_a_corrupt_row() -> Result<()> {
    // Arrange
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
        tag_order: vec![TagId::from_uuid(Uuid::now_v7())],
        context: None,
        agents_prepare: false,
        origin_id: None,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now).await?;
    let cleared = Heap {
        tag_order: Vec::new(),
        ..heap.clone()
    };

    // Act
    let saved = store::heaps::update_heap(&db, &cleared, Version::INITIAL).await?;
    let read = store::heaps::find_heap(&db, heap.id).await?;
    let stored_null = db
        .query_one_raw(store::Statement::from_string(
            db.get_database_backend(),
            format!(
                "SELECT COUNT(*) AS n FROM heaps WHERE id = '{}' AND tag_order IS NULL",
                heap.id.as_uuid().hyphenated()
            ),
        ))
        .await?
        .context("a row")?
        .try_get::<i64>("", "n")?;
    db.execute_unprepared(&format!(
        "UPDATE heaps SET tag_order = 'not json' WHERE id = '{}'",
        heap.id.as_uuid().hyphenated()
    ))
    .await?;
    let corrupt = store::heaps::find_heap(&db, heap.id).await;

    // Assert
    expect_that!(saved.map(|h| h.tag_order), some(is_empty()));
    expect_that!(read.map(|h| h.tag_order), some(is_empty()));
    expect_that!(stored_null, eq(1));
    expect_that!(corrupt, err(matches_pattern!(StoreError::CorruptRow { .. })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_context_round_trips_through_insert_and_update() -> Result<()> {
    // Arrange
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
        tag_order: Vec::new(),
        context: HeapContext::parse("Line one\n\tline two")?,
        agents_prepare: false,
        origin_id: None,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now).await?;

    // Act
    let inserted = store::heaps::find_heap(&db, heap.id).await?;
    let cleared = Heap {
        context: None,
        ..heap.clone()
    };
    let saved = store::heaps::update_heap(&db, &cleared, heap.version).await?;
    let after_clear = store::heaps::find_heap(&db, heap.id).await?;

    // Assert
    expect_that!(inserted, some(eq(&heap)));
    expect_that!(saved.map(|h| h.context), some(none()));
    expect_that!(after_clear.map(|h| h.context), some(none()));
    Ok(())
}
