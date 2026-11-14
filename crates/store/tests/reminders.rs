//! When a reminder joins the list (`items.arrives_at`): the migration on a database that already has items, the round
//! trip, and the open counts leaving out reminders to come.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Due, Item, ItemPatch, Kind, Need, NewItem, StateKind, Title};
use domain::team::Team;
use domain::text::Name;
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, ItemId, TeamId, Timestamp, UserId, Version};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, DbBackend};
use uuid::Uuid;

/// Migrations before `items.arrives_at`.
const BEFORE_ARRIVES_AT: u32 = 23;

fn at(millis: i64) -> Result<Timestamp> {
    Timestamp::from_millis(millis).context("in range")
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

#[gtest]
#[tokio::test]
async fn existing_items_have_no_arrival() -> Result<()> {
    // Arrange
    let db = store::testing::database_before_migration(BEFORE_ARRIVES_AT).await?;
    for sql in [
        format!(
            "INSERT INTO teams (id, slug, name, created_at, version) VALUES ('{}', 'infra', 'Infra', 0, 1)",
            id(1)
        ),
        format!(
            "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, need_weight, \
             created_at, version) VALUES ('{}', '{}', 'backlog', 'Backlog', 10, 10, 10, 10, 0, 1)",
            id(2),
            id(1)
        ),
    ] {
        db.execute_unprepared(&sql).await.with_context(|| sql.clone())?;
    }
    // `key` is reserved on MySQL, which quotes it with backticks; SQLite and Postgres take double quotes.
    let key = match db.get_database_backend() {
        DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    db.execute_unprepared(&format!(
        "INSERT INTO items (id, heap_id, {key}, title, state, need, created_at, version) \
         VALUES ('{}', '{}', 'k3x9a', 'Item', 'open', 'expected', 0, 1)",
        id(3),
        id(2)
    ))
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let item = store::items::find_item(&db, ItemId::from_uuid(Uuid::from_u128(3)))
        .await?
        .context("item")?;
    expect_that!(item.kind, eq(Kind::Note));
    expect_that!(item.arrives_at, none());
    Ok(())
}

struct World {
    db: DatabaseConnection,
    user: UserId,
    heap: HeapId,
}

async fn world() -> Result<World> {
    let db = store::testing::fresh_database().await?;
    let user = User {
        id: UserId::from_uuid(Uuid::now_v7()),
        username: Handle::parse("sam", Field::Username)?,
        email: Email::parse("sam@example.com")?,
        display_name: DisplayName::parse("Sam")?,
        created_at: at(0)?,
        version: Version::INITIAL,
    };
    store::users::insert_user(&db, &user).await?;
    let team = Team {
        id: TeamId::from_uuid(Uuid::now_v7()),
        slug: Handle::parse("team", Field::Slug)?,
        name: Name::parse("Team")?,
        personal_user: None,
        created_at: at(0)?,
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
    store::heaps::insert_heap(&db, &heap, at(0)?).await?;
    Ok(World {
        db,
        user: user.id,
        heap: heap.id,
    })
}

impl World {
    /// Adds an item of `kind`; a reminder arrives at `arrives_at` and is due a day later.
    async fn add(&self, key: &str, kind: Kind, arrives_at: Option<i64>) -> Result<Item> {
        let fields = NewItem {
            kind,
            title: Title::parse("An item")?,
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: arrives_at
                .map(|millis| {
                    Ok::<_, anyhow::Error>(Due {
                        at: at(millis + 86_400_000 - 1)?,
                        firm: true,
                    })
                })
                .transpose()?,
            arrives_at: arrives_at.map(at).transpose()?,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
            origin: None,
            blocked_by: None,
        };
        let item = Item::new(
            ItemId::from_uuid(Uuid::now_v7()),
            self.heap,
            ItemKey::parse(key)?,
            fields,
            self.user,
            at(0)?,
        );
        store::items::insert_item(&self.db, &item).await?;
        Ok(item)
    }

    async fn count(&self, now: i64) -> Result<u32> {
        let counts = store::items::count_items_by_heap(&self.db, &[self.heap], &StateKind::OPEN, at(now)?).await?;
        Ok(counts.get(&self.heap).copied().unwrap_or(0))
    }
}

#[gtest]
#[tokio::test]
async fn a_reminders_arrival_reads_back() -> Result<()> {
    // Arrange
    let w = world().await?;
    let added = w.add("k3x9a", Kind::Reminder, Some(5_000)).await?;

    // Act
    let read = store::items::find_item(&w.db, added.id).await?.context("item")?;

    // Assert
    expect_that!(read.kind, eq(Kind::Reminder));
    expect_that!(read.arrives_at, some(eq(at(5_000)?)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_that_was_a_reminder_keeps_the_arrival_it_was_given() -> Result<()> {
    // Arrange
    let w = world().await?;
    let reminder = w.add("k3x9a", Kind::Reminder, Some(5_000)).await?;
    let task = reminder
        .apply_patch(
            &ItemPatch {
                kind: Some(Kind::Task),
                ..ItemPatch::default()
            },
            w.user,
            at(1)?,
        )?
        .context("changed")?;

    // Act
    store::items::update_item(&w.db, &task, reminder.version, at(1)?)
        .await?
        .context("saved")?;
    let read = store::items::find_item(&w.db, reminder.id).await?.context("item")?;

    // Assert
    expect_that!(read.kind, eq(Kind::Task));
    expect_that!(read.arrives_at, some(eq(at(1)?)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_to_come_is_not_counted_and_one_that_arrived_is() -> Result<()> {
    // Arrange: a note, a reminder for later, and a task that was a reminder for later (a task counts at once).
    let w = world().await?;
    w.add("k3x9b", Kind::Note, None).await?;
    w.add("k3x9c", Kind::Reminder, Some(5_000)).await?;
    let converted = w.add("k3x9d", Kind::Reminder, Some(9_000)).await?;
    let task = converted
        .apply_patch(
            &ItemPatch {
                kind: Some(Kind::Task),
                ..ItemPatch::default()
            },
            w.user,
            at(1)?,
        )?
        .context("changed")?;
    store::items::update_item(&w.db, &task, converted.version, at(1)?)
        .await?
        .context("saved")?;

    // Act
    let before = w.count(4_999).await?;
    let on_the_day = w.count(5_000).await?;
    let after_both = w.count(9_000).await?;

    // Assert
    expect_that!(before, eq(2));
    expect_that!(on_the_day, eq(3));
    expect_that!(after_both, eq(3));
    Ok(())
}
