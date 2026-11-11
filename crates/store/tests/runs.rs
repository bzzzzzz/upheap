//! Run storage: the round trip, ending a run once, the cost, hearing from a run, counts and failures, and the
//! failures a turn has had. Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Item, Need, NewItem, Title};
use domain::run::{Run, RunEnd, RunLink, RunResult};
use domain::team::Team;
use domain::text::Name;
use domain::turn::{Reach, Turn, TurnSource};
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, RunId, TeamId, Timestamp, TurnId, UserId, Version};
use googletest::prelude::*;
use store::DatabaseConnection;
use uuid::Uuid;

fn at(millis: i64) -> Result<Timestamp> {
    Timestamp::from_millis(millis).context("in range")
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
        agents_prepare: false,
        archived_at: None,
        version: Version::INITIAL,
    };
    store::heaps::insert_heap(&db, &heap, at(1_000)?).await?;
    Ok(World {
        db,
        heap,
        user: user.id,
    })
}

/// An item with an open turn, and a run started on it at `started`.
async fn run_on_new_item(world: &World, key: &str, started: i64) -> Result<(Item, Turn, Run)> {
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
        domain::ItemId::from_uuid(Uuid::now_v7()),
        world.heap.id,
        ItemKey::parse(key)?,
        fields,
        world.user,
        at(1_000)?,
    );
    store::items::insert_item(&world.db, &item).await?;
    let turn = Turn::open(
        TurnId::from_uuid(Uuid::now_v7()),
        &item,
        Reach::One,
        None,
        TurnSource::GoAhead,
        world.user,
        at(1_000)?,
    );
    store::turns::insert_turn(&world.db, &turn).await?;
    let run = Run::start(
        RunId::from_uuid(Uuid::now_v7()),
        &item,
        turn.id,
        world.user,
        at(started)?,
    );
    store::runs::insert_run(&world.db, &run).await?;
    Ok((item, turn, run))
}

#[gtest]
#[tokio::test]
async fn a_new_run_reads_back_as_started() -> Result<()> {
    // Arrange
    let world = world().await?;

    // Act
    let (_, _, run) = run_on_new_item(&world, "k3x9a", 5_000).await?;
    let found = store::runs::find_run_locking(&world.db, run.id).await?;
    let plain = store::runs::find_run(&world.db, run.id).await?;
    let missing = store::runs::find_run(&world.db, RunId::from_uuid(Uuid::now_v7())).await?;

    // Assert
    expect_that!(found, some(eq(&run)));
    expect_that!(plain, some(eq(&run)));
    expect_that!(missing, none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_ends_once_with_its_result_and_links() -> Result<()> {
    // Arrange
    let world = world().await?;
    let (_, _, run) = run_on_new_item(&world, "k3x9a", 5_000).await?;
    let reached = run.ended_as(
        RunEnd::Reached,
        at(9_000)?,
        Some(RunResult::parse("Added the context")?),
        vec![RunLink::parse("https://example.com/pr/1")?],
        None,
    );
    let failed = run.ended_as(RunEnd::Failed, at(9_500)?, None, Vec::new(), Some("boom".to_owned()));

    // Act
    let first = store::runs::end_run(&world.db, &reached).await?;
    let second = store::runs::end_run(&world.db, &failed).await?;
    let found = store::runs::find_run(&world.db, run.id).await?;

    // Assert
    expect_that!(first, eq(true));
    expect_that!(second, eq(false));
    expect_that!(found, some(eq(&reached)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_cost_is_set_once_and_an_ended_run_is_not_touched() -> Result<()> {
    // Arrange
    let world = world().await?;
    let (_, _, run) = run_on_new_item(&world, "k3x9a", 5_000).await?;

    // Act
    let first = store::runs::set_run_cost(&world.db, run.id, 310_000).await?;
    let second = store::runs::set_run_cost(&world.db, run.id, 999).await?;
    store::runs::touch_run(&world.db, run.id, at(6_000)?).await?;
    let touched = store::runs::find_run(&world.db, run.id).await?.context("run")?;
    store::runs::end_run(
        &world.db,
        &run.ended_as(RunEnd::Asked, at(7_000)?, None, Vec::new(), None),
    )
    .await?;
    store::runs::touch_run(&world.db, run.id, at(8_000)?).await?;
    let ended = store::runs::find_run(&world.db, run.id).await?.context("run")?;

    // Assert
    expect_that!(first, eq(true));
    expect_that!(second, eq(false));
    expect_that!(touched.cost_micro_usd, some(eq(310_000)));
    expect_that!(touched.heard_at, eq(at(6_000)?));
    // A run that ended is no longer heard from.
    expect_that!(ended.heard_at, eq(at(6_000)?));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn touching_a_run_never_moves_it_back() -> Result<()> {
    // Arrange
    let world = world().await?;
    let (_, _, run) = run_on_new_item(&world, "k3x9a", 5_000).await?;

    // Act
    store::runs::touch_run(&world.db, run.id, at(4_000)?).await?;
    let found = store::runs::find_run(&world.db, run.id).await?.context("run")?;

    // Assert
    expect_that!(found.heard_at, eq(at(5_000)?));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_runs_that_have_not_ended_are_listed_by_heap_and_turn() -> Result<()> {
    // Arrange: a live run, a run that ended, and a run of another heap's item.
    let world = world().await?;
    let (_, turn, live) = run_on_new_item(&world, "k3x9a", 5_000).await?;
    let (_, _, ended) = run_on_new_item(&world, "m4y8b", 5_000).await?;
    store::runs::end_run(
        &world.db,
        &ended.ended_as(RunEnd::Failed, at(6_000)?, None, Vec::new(), Some("boom".to_owned())),
    )
    .await?;

    // Act
    let in_heap = store::runs::list_unended_runs_in_heaps(&world.db, &[world.heap.id], true).await?;
    let elsewhere =
        store::runs::list_unended_runs_in_heaps(&world.db, &[HeapId::from_uuid(Uuid::now_v7())], false).await?;
    let of_turn = store::runs::list_unended_runs_for_turn_locking(&world.db, turn.id).await?;

    // Assert
    expect_that!(in_heap, elements_are![eq(&live)]);
    expect_that!(elsewhere, is_empty());
    expect_that!(of_turn, elements_are![eq(&live)]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn runs_are_counted_by_turn_and_the_latest_failure_is_found() -> Result<()> {
    // Arrange: a turn with two failed runs and one that asked, and another turn with none.
    let world = world().await?;
    let (item, turn, first) = run_on_new_item(&world, "k3x9a", 5_000).await?;
    let (_, quiet, _) = run_on_new_item(&world, "m4y8b", 5_000).await?;
    let second = Run::start(RunId::from_uuid(Uuid::now_v7()), &item, turn.id, world.user, at(7_000)?);
    let third = Run::start(RunId::from_uuid(Uuid::now_v7()), &item, turn.id, world.user, at(9_000)?);
    store::runs::insert_run(&world.db, &second).await?;
    store::runs::insert_run(&world.db, &third).await?;
    for (run, end, at_ms, failure) in [
        (&first, RunEnd::Failed, 6_000, Some("first failure")),
        (&second, RunEnd::Failed, 8_000, Some("second failure")),
        (&third, RunEnd::Asked, 10_000, None),
    ] {
        store::runs::end_run(
            &world.db,
            &run.ended_as(end, at(at_ms)?, None, Vec::new(), failure.map(str::to_owned)),
        )
        .await?;
    }

    // Act
    let counts = store::runs::count_runs_for_turns(&world.db, &[turn.id, quiet.id]).await?;
    let failures = store::runs::latest_failure_for_turns(&world.db, &[turn.id, quiet.id]).await?;

    // Assert
    expect_that!(counts.get(&turn.id), some(eq(&3)));
    expect_that!(counts.get(&quiet.id), some(eq(&1)));
    expect_that!(failures.get(&turn.id).map(String::as_str), some(eq("second failure")));
    expect_that!(failures.get(&quiet.id), none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turns_failed_runs_are_stored_and_only_an_open_turn_takes_them() -> Result<()> {
    // Arrange
    let world = world().await?;
    let (item, turn, _) = run_on_new_item(&world, "k3x9a", 5_000).await?;

    // Act
    let set = store::turns::set_failed_runs(&world.db, turn.id, 3).await?;
    let found = store::turns::find_open_turn(&world.db, item.id)
        .await?
        .context("turn")?;
    store::turns::end_turn(&world.db, &found.reached(world.user, at(9_000)?)).await?;
    let late = store::turns::set_failed_runs(&world.db, turn.id, 0).await?;
    let ended = store::turns::find_turn(&world.db, turn.id).await?.context("turn")?;
    let in_heap = store::turns::list_open_turns_in_heaps(&world.db, &[world.heap.id], true).await?;

    // Assert
    expect_that!(set, eq(true));
    expect_that!(found.failed_runs, eq(3));
    expect_that!(found.is_stuck(), eq(true));
    expect_that!(late, eq(false));
    expect_that!(ended.failed_runs, eq(3));
    expect_that!(ended.is_open(), eq(false));
    expect_that!(in_heap, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_question_remembers_the_run_it_was_asked_in() -> Result<()> {
    use domain::QuestionId;
    use domain::question::{NewQuestion, Question, QuestionKind, QuestionNumber, QuestionText};

    // Arrange
    let world = world().await?;
    let (item, turn, run) = run_on_new_item(&world, "k3x9a", 5_000).await?;
    let fields = |text: &str| -> Result<NewQuestion> {
        Ok(NewQuestion::new(
            QuestionKind::Question,
            QuestionText::parse(text)?,
            None,
            &[],
            false,
            None,
        )?)
    };
    let new_question = |number: u32, text: &str, run: Option<RunId>| -> Result<Question> {
        Ok(Question::new(
            QuestionId::from_uuid(Uuid::now_v7()),
            item.id,
            QuestionNumber::new(number).context("number")?,
            fields(text)?,
            world.user,
            at(6_000).context("time")?,
        )
        .asked_in(run))
    };
    let agents = new_question(1, "Which CA?", Some(run.id))?;
    let person = new_question(2, "Mine", None)?;
    store::questions::insert_question(&world.db, &agents).await?;
    store::questions::insert_question(&world.db, &person).await?;

    // Act
    let listed = store::questions::list_questions(&world.db, item.id).await?;
    let of_turn = store::questions::list_open_questions_for_runs_of_turn_locking(&world.db, turn.id).await?;
    let asked = store::questions::count_questions_in_run(&world.db, run.id).await?;

    // Assert
    expect_that!(listed, elements_are![eq(&agents), eq(&person)]);
    expect_that!(of_turn, elements_are![eq(&agents)]);
    expect_that!(asked, eq(1));
    Ok(())
}
