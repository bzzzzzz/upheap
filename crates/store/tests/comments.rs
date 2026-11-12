//! Comment storage: the round trip, the order and the long text.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use domain::comment::{Comment, CommentBody, CommentNumber};
use domain::handle::{Handle, ItemKey};
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Item, Kind, Need, NewItem, Title};
use domain::team::Team;
use domain::text::Name;
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{CommentId, HeapId, ItemId, TeamId, Timestamp, UserId, Version};
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
        heap.id,
        ItemKey::parse("k3x9a")?,
        fields,
        user.id,
        now()?,
    );
    store::items::insert_item(&db, &item).await?;
    Ok((db, item.id, user.id))
}

fn comment(item_id: ItemId, author: UserId, number: u32, body: &str) -> Result<Comment> {
    Ok(Comment::new(
        CommentId::from_uuid(Uuid::now_v7()),
        item_id,
        CommentNumber::new(number).context("positive")?,
        CommentBody::parse(body)?,
        author,
        now()?,
    ))
}

#[gtest]
#[tokio::test]
async fn comments_read_back_in_number_order_as_written() -> Result<()> {
    // Arrange
    let (db, item_id, author) = item_in_new_database().await?;
    let first = comment(item_id, author, 1, "Use the internal CA\nfor staging.")?;
    let second = comment(item_id, author, 2, "Done.")?;
    let third = comment(item_id, author, 3, "Production too.")?;

    // Act
    store::comments::insert_comment(&db, &third).await?;
    store::comments::insert_comment(&db, &first).await?;
    store::comments::insert_comment(&db, &second).await?;
    let listed = store::comments::list_comments(&db, item_id).await?;
    let locked = store::comments::list_comments_locking(&db, item_id).await?;

    // Assert
    expect_that!(listed, elements_are![eq(&first), eq(&second), eq(&third)]);
    expect_that!(locked, eq(&listed));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_number_is_used_once_per_item() -> Result<()> {
    // Arrange
    let (db, item_id, author) = item_in_new_database().await?;
    store::comments::insert_comment(&db, &comment(item_id, author, 1, "One")?).await?;

    // Act
    let again = store::comments::insert_comment(&db, &comment(item_id, author, 1, "Other")?).await;

    // Assert
    expect_that!(again, err(anything()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn twenty_thousand_four_byte_characters_fit_in_a_comment() -> Result<()> {
    // Arrange: 80,000 bytes, more than MySQL's TEXT holds.
    let long = "\u{1F600}".repeat(20_000);
    let (db, item_id, author) = item_in_new_database().await?;

    // Act
    store::comments::insert_comment(&db, &comment(item_id, author, 1, &long)?).await?;
    let listed = store::comments::list_comments(&db, item_id).await?;

    // Assert
    expect_that!(
        listed
            .iter()
            .map(|c| c.body.as_str().chars().count())
            .collect::<Vec<_>>(),
        elements_are![eq(&20_000)]
    );
    Ok(())
}
