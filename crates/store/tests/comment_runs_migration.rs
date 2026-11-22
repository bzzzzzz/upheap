//! The comment runs migration on a database that already has comments: a comment an agent wrote in a run gets that
//! run from its audit entry, and a person's comment keeps none.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Item, Kind, Need, NewItem, Title};
use domain::run::Run;
use domain::team::Team;
use domain::text::Name;
use domain::turn::{Reach, Turn, TurnSource, goal_for};
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, ItemId, RunId, TeamId, Timestamp, TurnId, UserId, Version};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection};
use uuid::Uuid;

/// Migrations before the comment runs one.
const BEFORE_COMMENT_RUNS: u32 = 54;

fn at(millis: i64) -> Result<Timestamp> {
    Timestamp::from_millis(millis).context("in range")
}

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

/// A comment as the schema before the migration holds it, with the audit entry written beside it: about the same
/// item, by the same user, at the same time, and in `run` when an agent wrote it.
async fn comment_before(
    db: &DatabaseConnection,
    heap: &Heap,
    item: &Item,
    user: UserId,
    number: u32,
    created_at: i64,
    run_id: Option<RunId>,
) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO item_comments (id, item_id, number, body, created_by, created_at) \
             VALUES ('{}', '{}', {number}, 'A comment', '{user}', {created_at})",
            Uuid::now_v7().hyphenated(),
            item.id,
        ),
    )
    .await?;
    let run_value = run_id.map_or_else(|| "NULL".to_owned(), |id| format!("'{id}'"));
    run(
        db,
        &format!(
            "INSERT INTO audit_log \
             (id, team_id, heap_id, actor_user_id, action, entity_type, entity_id, diff, run_id, created_at) \
             VALUES ('{}', '{}', '{}', '{user}', 'comment_added', 'item', '{}', '{{}}', {run_value}, {created_at})",
            Uuid::now_v7().hyphenated(),
            heap.team_id,
            heap.id,
            item.id,
        ),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn existing_comments_get_the_run_of_their_audit_entry() -> Result<()> {
    // Arrange: an item with a run on it, before comments know their run. The agent's comment is the first, a
    // person's the second, and a third has no audit entry at all.
    let db = store::testing::database_before_migration(BEFORE_COMMENT_RUNS).await?;
    let user = User {
        id: UserId::from_uuid(Uuid::now_v7()),
        username: Handle::parse("sam", Field::Username)?,
        email: Email::parse("sam@example.com")?,
        display_name: DisplayName::parse("Sam")?,
        created_at: at(1_000)?,
        version: Version::INITIAL,
    };
    store::users::insert_user(&db, &user).await?;
    let team = Team {
        id: TeamId::from_uuid(Uuid::now_v7()),
        slug: Handle::parse("team", Field::Slug)?,
        name: Name::parse("Team")?,
        personal_user: None,
        created_at: at(1_000)?,
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
        tag_order: Vec::new(),
        agents_prepare: false,
        origin_id: None,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, at(1_000)?).await?;
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
        at(1_000)?,
    );
    store::items::insert_item(&db, &item).await?;
    let turn = Turn::open(
        TurnId::from_uuid(Uuid::now_v7()),
        &item,
        Reach::One,
        goal_for(&item),
        None,
        TurnSource::GoAhead,
        user.id,
        at(1_000)?,
    );
    store::turns::insert_turn(&db, &turn).await?;
    let agent_run = Run::start(RunId::from_uuid(Uuid::now_v7()), &item, turn.id, user.id, at(2_000)?);
    store::runs::insert_run(&db, &agent_run).await?;
    comment_before(&db, &heap, &item, user.id, 1, 3_000, Some(agent_run.id)).await?;
    comment_before(&db, &heap, &item, user.id, 2, 4_000, None).await?;
    run(
        &db,
        &format!(
            "INSERT INTO item_comments (id, item_id, number, body, created_by, created_at) \
             VALUES ('{}', '{}', 3, 'A comment', '{}', 5000)",
            Uuid::now_v7().hyphenated(),
            item.id,
            user.id,
        ),
    )
    .await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let runs: Vec<Option<RunId>> = store::comments::list_comments(&db, item.id)
        .await?
        .into_iter()
        .map(|comment| comment.run_id)
        .collect();
    expect_that!(runs, eq(&vec![Some(agent_run.id), None, None]));
    Ok(())
}
