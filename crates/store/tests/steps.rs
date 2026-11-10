//! Step storage: the round trip, the version check and making room for a step inserted in the middle.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Item, Need, NewItem, Title};
use domain::step::{NewStep, Step, StepContext, StepNumber, StepPatch, StepPlacement, StepProgress, StepState};
use domain::team::Team;
use domain::text::Name;
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, ItemId, StepId, TeamId, Timestamp, UserId, Version};
use googletest::prelude::*;
use store::DatabaseConnection;
use uuid::Uuid;

fn now() -> Result<Timestamp> {
    Timestamp::from_millis(0).context("in range")
}

async fn item_in_new_database() -> Result<(DatabaseConnection, ItemId)> {
    let db = store::testing::fresh_database().await?;
    let user = User {
        id: UserId::from_uuid(Uuid::now_v7()),
        username: Handle::parse("sam", Field::Username)?,
        email: Email::parse("sam@example.com")?,
        display_name: DisplayName::parse("Sam")?,
        created_at: now()?,
        version: Version::INITIAL,
    };
    store::users::insert_user(&db, &user).await?;
    let team = Team {
        id: TeamId::from_uuid(Uuid::now_v7()),
        slug: Handle::parse("team", Field::Slug)?,
        name: Name::parse("Team")?,
        personal_user: None,
        created_at: now()?,
        version: Version::INITIAL,
    };
    store::teams::insert_team(&db, &team).await?;
    let heap = Heap {
        id: HeapId::from_uuid(Uuid::now_v7()),
        team_id: team.id,
        slug: Handle::parse("heap", Field::Slug)?,
        name: Name::parse("Heap")?,
        policy: HeapPolicy::default(),
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now()?).await?;
    let fields = NewItem {
        title: Title::parse("Rotate the certs")?,
        description: None,
        need: Need::Expected,
        effort: None,
        impact: None,
        due: None,
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
    };
    let item = Item::new(
        ItemId::from_uuid(Uuid::now_v7()),
        heap.id,
        ItemKey::parse("k3x9a")?,
        fields,
        user.id,
        now()?,
    );
    store::items::insert_item(&db, &item).await?;
    Ok((db, item.id))
}

fn step(item_id: ItemId, number: u32, position: u32, title: &str) -> Result<Step> {
    let fields = NewStep {
        title: Title::parse(title)?,
        context: None,
        after: None,
    };
    let placement = StepPlacement {
        number: StepNumber::new(number).context("positive")?,
        position,
    };
    Ok(Step::new(
        StepId::from_uuid(Uuid::now_v7()),
        item_id,
        placement,
        fields,
        now()?,
    ))
}

#[gtest]
#[tokio::test]
async fn steps_read_back_in_position_order_as_written() -> Result<()> {
    // Arrange
    let (db, item_id) = item_in_new_database().await?;
    let mut second = step(item_id, 2, 1, "Run in staging")?;
    second.context = StepContext::parse("Use the blue stack")?;

    // Act
    store::steps::insert_step(&db, &second).await?;
    store::steps::insert_step(&db, &step(item_id, 1, 0, "Inventory the certs")?).await?;
    let listed = store::steps::list_steps(&db, item_id).await?;

    // Assert
    expect_that!(
        listed.iter().map(|s| s.number.get()).collect::<Vec<_>>(),
        elements_are![eq(&1), eq(&2)]
    );
    expect_that!(listed[1], eq(&second));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn progress_counts_ticked_of_open_and_ticked_steps_per_item() -> Result<()> {
    // Arrange
    let (db, item_id) = item_in_new_database().await?;
    for (number, state) in [
        (1, StepState::Ticked),
        (2, StepState::Open),
        (3, StepState::Ticked),
        (4, StepState::Dropped),
    ] {
        let mut row = step(item_id, number, number - 1, "A step")?;
        row.state = state;
        store::steps::insert_step(&db, &row).await?;
    }
    let other = ItemId::from_uuid(Uuid::now_v7());

    // Act
    let progress = store::steps::count_step_progress(&db, &[item_id, other]).await?;

    // Assert
    expect_that!(progress.get(&item_id), some(eq(&StepProgress { ticked: 2, total: 3 })));
    expect_that!(progress, len(eq(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_update_needs_the_stored_version() -> Result<()> {
    // Arrange
    let (db, item_id) = item_in_new_database().await?;
    let first = step(item_id, 1, 0, "Inventory the certs")?;
    store::steps::insert_step(&db, &first).await?;
    let ticked = first
        .apply_patch(&StepPatch {
            state: Some(StepState::Ticked),
            ..StepPatch::default()
        })
        .context("changed")?;

    // Act
    let stale = store::steps::update_step(&db, &ticked, Version::new(7)).await?;
    let saved = store::steps::update_step(&db, &ticked, first.version).await?;

    // Assert
    expect_that!(stale, none());
    expect_that!(
        saved.map(|s| (s.state, s.version)),
        some(eq((StepState::Ticked, Version::new(2))))
    );
    let stored = store::steps::list_steps(&db, item_id).await?;
    expect_that!(stored[0].state, eq(StepState::Ticked));
    expect_that!(stored[0].version, eq(Version::new(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn shifting_makes_room_after_a_position() -> Result<()> {
    // Arrange
    let (db, item_id) = item_in_new_database().await?;
    store::steps::insert_step(&db, &step(item_id, 1, 0, "One")?).await?;
    store::steps::insert_step(&db, &step(item_id, 2, 1, "Two")?).await?;

    // Act
    store::steps::shift_positions_after(&db, item_id, 0).await?;
    store::steps::insert_step(&db, &step(item_id, 3, 1, "Three")?).await?;
    let listed = store::steps::list_steps(&db, item_id).await?;

    // Assert
    expect_that!(
        listed.iter().map(|s| (s.number.get(), s.position)).collect::<Vec<_>>(),
        elements_are![eq(&(1, 0)), eq(&(3, 1)), eq(&(2, 2))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_number_is_unique_within_its_item() -> Result<()> {
    // Arrange
    let (db, item_id) = item_in_new_database().await?;
    store::steps::insert_step(&db, &step(item_id, 1, 0, "One")?).await?;

    // Act
    let again = store::steps::insert_step(&db, &step(item_id, 1, 1, "Again")?).await;

    // Assert
    expect_that!(again, err(anything()));
    Ok(())
}
