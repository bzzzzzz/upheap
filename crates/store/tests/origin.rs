//! Item origins: the link to the item an item was made from, and the lookup of what was made from an item.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Age, Item, ItemState, Kind, Need, NewItem, NewOrigin, Title};
use domain::team::Team;
use domain::text::Name;
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, ItemId, TeamId, Timestamp, UserId, Version};
use googletest::prelude::*;
use store::DatabaseConnection;
use uuid::Uuid;

fn now() -> Result<Timestamp> {
    Timestamp::from_millis(0).context("in range")
}

async fn item_in_new_database() -> Result<(DatabaseConnection, Item, UserId)> {
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
        context: None,
        agents_prepare: false,
        origin_id: None,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now()?).await?;
    let fields = NewItem {
        kind: Kind::Note,
        title: Title::parse("Rotate the certs")?,
        description: None,
        need: Need::Expected,
        effort: None,
        impact: None,
        due: None,
        arrives_at: None,
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        origin: None,
        blocked_by: None,
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
    Ok((db, item, user.id))
}

fn made_from(note: &Item, key: &str, by: UserId, ms: i64, blocked_by: Option<ItemId>) -> Result<Item> {
    let fields = NewItem {
        kind: Kind::Task,
        title: Title::parse("Rotate staging")?,
        description: None,
        need: Need::Expected,
        effort: None,
        impact: None,
        due: None,
        arrives_at: Some(Timestamp::from_millis(0).context("in range")?),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        origin: Some(NewOrigin {
            item_id: note.id,
            age: Age::Keep,
        }),
        blocked_by,
    };
    Ok(Item::new(
        ItemId::from_uuid(Uuid::now_v7()),
        note.heap_id,
        ItemKey::parse(key)?,
        fields,
        by,
        Timestamp::from_millis(ms).context("in range")?,
    ))
}

#[gtest]
#[tokio::test]
async fn items_made_from_a_note_read_back_oldest_first_with_their_origin() -> Result<()> {
    // Arrange
    let (db, note, user) = item_in_new_database().await?;
    let first = made_from(&note, "a1a1a", user, 10, None)?;
    let second = made_from(&note, "b2b2b", user, 20, Some(first.id))?;
    store::items::insert_item(&db, &first).await?;
    store::items::insert_item(&db, &second).await?;

    // Act
    let made = store::items::list_items_made_from(&db, &[note.id], false).await?;
    let none_made = store::items::list_items_made_from(&db, &[first.id], true).await?;

    // Assert
    expect_that!(
        made.iter().map(|item| item.id).collect::<Vec<_>>(),
        eq(&vec![first.id, second.id])
    );
    expect_that!(made[0].origin_id, some(eq(note.id)));
    expect_that!(
        made[1].state,
        eq(&ItemState::Blocked {
            by: Some(first.id),
            reason: None
        })
    );
    expect_that!(none_made, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_that_became_reads_back_as_became() -> Result<()> {
    // Arrange
    let (db, note, user) = item_in_new_database().await?;
    let closed_at = Timestamp::from_millis(5).context("in range")?;
    let became = note.became(user, closed_at)?.context("changed")?;

    // Act
    let saved = store::items::update_item(&db, &became, note.version, closed_at).await?;
    let read = store::items::find_item(&db, note.id).await?.context("stored")?;

    // Assert
    expect_that!(saved.is_some(), eq(true));
    expect_that!(
        read.state,
        eq(&ItemState::Became {
            closed_at,
            closed_by: Some(user)
        })
    );
    Ok(())
}
