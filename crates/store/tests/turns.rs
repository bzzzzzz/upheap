//! Turn storage: the round trip, ending a turn once, the open turns of a heap, and the heap's prepare setting.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Item, Kind, Need, NewItem, Title};
use domain::team::Team;
use domain::text::Name;
use domain::turn::{Reach, Turn, TurnLine, TurnSource};
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, ItemId, TeamId, Timestamp, TurnId, UserId, Version};
use googletest::prelude::*;
use store::DatabaseConnection;
use uuid::Uuid;

fn now() -> Result<Timestamp> {
    Timestamp::from_millis(1_000).context("in range")
}

struct World {
    db: DatabaseConnection,
    heap: Heap,
    user: UserId,
}

async fn world() -> Result<World> {
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
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, now()?).await?;
    Ok(World {
        db,
        heap,
        user: user.id,
    })
}

async fn item(world: &World, key: &str) -> Result<Item> {
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
    };
    let item = Item::new(
        ItemId::from_uuid(Uuid::now_v7()),
        world.heap.id,
        ItemKey::parse(key)?,
        fields,
        world.user,
        now()?,
    );
    store::items::insert_item(&world.db, &item).await?;
    Ok(item)
}

fn turn(item: &Item, user: UserId, line: Option<&str>) -> Result<Turn> {
    Ok(Turn::open(
        TurnId::from_uuid(Uuid::now_v7()),
        item,
        Reach::AllTheWay,
        line.map(TurnLine::parse).transpose()?.flatten(),
        TurnSource::GoAhead,
        user,
        now()?,
    ))
}

#[gtest]
#[tokio::test]
async fn an_open_turn_reads_back_as_written() -> Result<()> {
    // Arrange
    let world = world().await?;
    let item = item(&world, "k3x9a").await?;
    let opened = turn(&item, world.user, Some("Use the new CA.\nPlease."))?;

    // Act
    store::turns::insert_turn(&world.db, &opened).await?;
    let found = store::turns::find_open_turn(&world.db, item.id).await?;
    let locked = store::turns::find_open_turn_locking(&world.db, item.id).await?;
    let listed = store::turns::list_open_turns(&world.db, &[item.id]).await?;

    // Assert
    expect_that!(found, some(eq(&opened)));
    expect_that!(locked, some(eq(&opened)));
    expect_that!(listed.get(&item.id), some(eq(&opened)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_ends_once_and_is_then_not_open_but_stays() -> Result<()> {
    // Arrange
    let world = world().await?;
    let item = item(&world, "k3x9a").await?;
    let opened = turn(&item, world.user, None)?;
    store::turns::insert_turn(&world.db, &opened).await?;
    let ended = opened.take_back(world.user, now()?);

    // Act
    let first = store::turns::end_turn(&world.db, &ended).await?;
    let second = store::turns::end_turn(&world.db, &ended).await?;
    let found = store::turns::find_open_turn(&world.db, item.id).await?;
    let listed = store::turns::list_open_turns(&world.db, &[item.id]).await?;

    // Assert
    expect_that!(first, eq(true));
    expect_that!(second, eq(false));
    expect_that!(found, none());
    expect_that!(listed, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_open_turns_of_a_heap_are_listed_and_ended_ones_are_not() -> Result<()> {
    // Arrange
    let world = world().await?;
    let first = item(&world, "k3x9a").await?;
    let second = item(&world, "m4y8b").await?;
    let third = item(&world, "p5z7c").await?;
    let open = turn(&first, world.user, None)?;
    let other = turn(&second, world.user, Some("A line"))?;
    let past = turn(&third, world.user, None)?;
    for turn in [&open, &other, &past] {
        store::turns::insert_turn(&world.db, turn).await?;
    }
    store::turns::end_turn(&world.db, &past.take_back(world.user, now()?)).await?;

    // Act
    let in_heap = store::turns::list_open_turns_in_heap_locking(&world.db, world.heap.id).await?;
    let elsewhere = store::turns::list_open_turns_in_heap_locking(&world.db, HeapId::from_uuid(Uuid::now_v7())).await?;

    // Assert
    expect_that!(in_heap.len(), eq(2));
    expect_that!(in_heap, contains(eq(&open)));
    expect_that!(in_heap, contains(eq(&other)));
    expect_that!(elsewhere, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_prepare_setting_round_trips_through_insert_and_update() -> Result<()> {
    // Arrange
    let world = world().await?;
    let on = Heap {
        agents_prepare: true,
        ..world.heap.clone()
    };

    // Act
    let before = store::heaps::find_heap(&world.db, world.heap.id).await?;
    let saved = store::heaps::update_heap(&world.db, &on, world.heap.version).await?;
    let after = store::heaps::find_heap(&world.db, world.heap.id).await?;

    // Assert
    expect_that!(before.map(|heap| heap.agents_prepare), some(eq(false)));
    expect_that!(saved.map(|heap| heap.agents_prepare), some(eq(true)));
    expect_that!(after.map(|heap| heap.agents_prepare), some(eq(true)));
    Ok(())
}
