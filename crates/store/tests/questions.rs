//! Question storage: the round trip, the version check and the long texts.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Item, Need, NewItem, Title};
use domain::question::{NewQuestion, Question, QuestionKind, QuestionNumber, QuestionState, QuestionText};
use domain::step::StepNumber;
use domain::team::Team;
use domain::text::{Context, Name};
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, ItemId, QuestionId, TeamId, Timestamp, UserId, Version};
use googletest::prelude::*;
use store::DatabaseConnection;
use uuid::Uuid;

fn now() -> Result<Timestamp> {
    Timestamp::from_millis(0).context("in range")
}

async fn item_in_new_database() -> Result<(DatabaseConnection, ItemId, UserId)> {
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
    Ok((db, item.id, user.id))
}

fn question(
    item_id: ItemId,
    asker: UserId,
    number: u32,
    kind: QuestionKind,
    choices: &[&str],
    many: bool,
) -> Result<Question> {
    let choices: Vec<String> = choices.iter().map(|label| (*label).to_owned()).collect();
    let fields = NewQuestion::new(kind, QuestionText::parse("Which CA?")?, None, &choices, many, None)?;
    Ok(Question::new(
        QuestionId::from_uuid(Uuid::now_v7()),
        item_id,
        QuestionNumber::new(number).context("positive")?,
        fields,
        asker,
        now()?,
    ))
}

#[gtest]
#[tokio::test]
async fn questions_read_back_in_number_order_as_written() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let mut second = question(
        item_id,
        asker,
        2,
        QuestionKind::Question,
        &["Internal", "Let's Encrypt"],
        true,
    )?;
    second.details = Context::parse_as("The staging one", domain::validation::Field::Details)?;
    second.step = StepNumber::new(3);
    let third = question(item_id, asker, 3, QuestionKind::Request, &[], false)?;

    // Act
    store::questions::insert_question(&db, &third).await?;
    store::questions::insert_question(&db, &second).await?;
    store::questions::insert_question(&db, &question(item_id, asker, 1, QuestionKind::Question, &[], false)?).await?;
    let listed = store::questions::list_questions(&db, item_id).await?;
    let locked = store::questions::list_questions_locking(&db, item_id).await?;

    // Assert
    expect_that!(
        listed.iter().map(|q| q.number.get()).collect::<Vec<_>>(),
        elements_are![eq(&1), eq(&2), eq(&3)]
    );
    expect_that!(listed[1], eq(&second));
    expect_that!(listed[2], eq(&third));
    expect_that!(locked, eq(&listed));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reply_round_trips_and_moves_the_version() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let open = question(item_id, asker, 1, QuestionKind::Question, &["A", "B", "C"], true)?;
    store::questions::insert_question(&db, &open).await?;
    let answered = open.reply(&[2, 0], Context::parse("Both, please")?, asker, now()?)?;

    // Act
    let saved = store::questions::update_question(&db, &answered, open.version)
        .await?
        .context("written")?;
    let listed = store::questions::list_questions(&db, item_id).await?;

    // Assert
    expect_that!(saved.version, eq(open.version.next()));
    expect_that!(listed, elements_are![eq(&saved)]);
    expect_that!(listed[0].state(), eq(QuestionState::Answered));
    expect_that!(
        listed[0].reply.as_ref().map(|reply| reply.picked.clone()),
        some(eq(&vec![0, 2]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_version_writes_nothing() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let open = question(item_id, asker, 1, QuestionKind::Request, &[], false)?;
    store::questions::insert_question(&db, &open).await?;
    let answered = open.reply(&[0], None, asker, now()?)?;
    store::questions::update_question(&db, &answered, open.version).await?;

    // Act
    let again = store::questions::update_question(&db, &answered, open.version).await?;

    // Assert
    expect_that!(again, none());
    let listed = store::questions::list_questions(&db, item_id).await?;
    expect_that!(listed[0].version, eq(open.version.next()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_question_whose_asker_is_unknown_reads_back() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let mut anonymous = question(item_id, asker, 1, QuestionKind::Question, &[], false)?;
    anonymous.asked_by = None;
    store::questions::insert_question(&db, &anonymous).await?;

    // Act
    let listed = store::questions::list_questions(&db, item_id).await?;

    // Assert
    expect_that!(listed, elements_are![eq(&anonymous)]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn twenty_thousand_four_byte_characters_fit_in_details_and_a_comment() -> Result<()> {
    // Arrange: 80,000 bytes, more than MySQL's TEXT holds.
    let long = "\u{1F600}".repeat(20_000);
    let (db, item_id, asker) = item_in_new_database().await?;
    let mut open = question(item_id, asker, 1, QuestionKind::Question, &[], false)?;
    open.details = Context::parse_as(&long, domain::validation::Field::Details)?;
    store::questions::insert_question(&db, &open).await?;
    let answered = open.reply(
        &[],
        Context::parse_as(&long, domain::validation::Field::Comment)?,
        asker,
        now()?,
    )?;

    // Act
    store::questions::update_question(&db, &answered, open.version).await?;
    let listed = store::questions::list_questions(&db, item_id).await?;

    // Assert
    expect_that!(
        listed[0]
            .details
            .as_ref()
            .map(|details| details.as_str().chars().count()),
        some(eq(20_000))
    );
    expect_that!(
        listed[0]
            .reply
            .as_ref()
            .and_then(|reply| reply.comment.as_ref())
            .map(|comment| comment.as_str().chars().count()),
        some(eq(20_000))
    );
    Ok(())
}

/// Closes the item and returns the stored one.
async fn close(db: &DatabaseConnection, item_id: ItemId, by: UserId) -> Result<()> {
    let item = store::items::find_item(db, item_id).await?.context("item")?;
    let closed = Item {
        state: domain::item::ItemState::Done {
            closed_at: now()?,
            closed_by: Some(by),
        },
        ..item.clone()
    };
    store::items::update_item(db, &closed, item.version, now()?)
        .await?
        .context("saved")?;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_open_questions_query_skips_answered_questions_and_closed_items() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let heap_id = store::items::find_item(&db, item_id).await?.context("item")?.heap_id;
    let answered = question(item_id, asker, 1, QuestionKind::Question, &[], false)?;
    store::questions::insert_question(&db, &answered).await?;
    let reply = answered.reply(&[], Context::parse_as("Yes", Field::Comment)?, asker, now()?)?;
    store::questions::update_question(&db, &reply, answered.version)
        .await?
        .context("saved")?;
    store::questions::insert_question(&db, &question(item_id, asker, 2, QuestionKind::Request, &[], false)?).await?;
    store::questions::insert_question(&db, &question(item_id, asker, 3, QuestionKind::Question, &[], false)?).await?;

    // Act
    let open = store::questions::list_open_questions_on_open_items(&db, &[heap_id]).await?;
    let elsewhere =
        store::questions::list_open_questions_on_open_items(&db, &[HeapId::from_uuid(Uuid::now_v7())]).await?;
    close(&db, item_id, asker).await?;
    let closed = store::questions::list_open_questions_on_open_items(&db, &[heap_id]).await?;

    // Assert
    expect_that!(
        open.iter().map(|(heap, q)| (*heap, q.number.get())).collect::<Vec<_>>(),
        elements_are![eq(&(heap_id, 2)), eq(&(heap_id, 3))]
    );
    expect_that!(elsewhere, is_empty());
    expect_that!(closed, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn open_questions_are_counted_per_item() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let answered = question(item_id, asker, 1, QuestionKind::Question, &[], false)?;
    store::questions::insert_question(&db, &answered).await?;
    let reply = answered.reply(&[], Context::parse_as("Yes", Field::Comment)?, asker, now()?)?;
    store::questions::update_question(&db, &reply, answered.version)
        .await?
        .context("saved")?;
    store::questions::insert_question(&db, &question(item_id, asker, 2, QuestionKind::Request, &[], false)?).await?;
    store::questions::insert_question(&db, &question(item_id, asker, 3, QuestionKind::Question, &[], false)?).await?;
    let other = ItemId::from_uuid(Uuid::now_v7());

    // Act
    let counts = store::questions::count_open_questions(&db, &[item_id, other]).await?;
    let none = store::questions::count_open_questions(&db, &[]).await?;

    // Assert
    expect_that!(counts.get(&item_id), some(eq(&2)));
    expect_that!(counts.contains_key(&other), eq(false));
    expect_that!(none, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_withdrawn_question_round_trips_and_is_no_longer_open() -> Result<()> {
    // Arrange
    let (db, item_id, asker) = item_in_new_database().await?;
    let heap_id = store::items::find_item(&db, item_id).await?.context("item")?.heap_id;
    let open = question(item_id, asker, 1, QuestionKind::Question, &[], false)?;
    store::questions::insert_question(&db, &open).await?;
    store::questions::insert_question(&db, &question(item_id, asker, 2, QuestionKind::Question, &[], false)?).await?;
    let withdrawn = open.withdraw(asker, now()?);

    // Act
    let saved = store::questions::update_question(&db, &withdrawn, open.version)
        .await?
        .context("saved")?;
    let listed = store::questions::list_questions(&db, item_id).await?;
    let counts = store::questions::count_open_questions(&db, &[item_id]).await?;
    let waiting = store::questions::list_open_questions_on_open_items(&db, &[heap_id]).await?;

    // Assert
    expect_that!(saved.version, eq(open.version.next()));
    expect_that!(listed[0], eq(&saved));
    expect_that!(listed[0].state(), eq(QuestionState::Withdrawn));
    expect_that!(listed[0].reply, none());
    expect_that!(counts.get(&item_id), some(eq(&1)));
    expect_that!(
        waiting.iter().map(|(_, q)| q.number.get()).collect::<Vec<_>>(),
        elements_are![eq(&2)]
    );
    Ok(())
}
