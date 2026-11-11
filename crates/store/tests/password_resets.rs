//! Password reset links and setting a password. Runs on in-memory SQLite, or on MySQL or Postgres with
//! `TEST_DATABASE_URL` set; those runs prove the conditional updates and the foreign key on each backend.

use anyhow::{Context, Result};
use domain::handle::Handle;
use domain::password_reset::{RESET_LINK_LIFETIME, ResetLink, ResetLinkState};
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{Duration, Timestamp, UserId, Version};
use googletest::prelude::*;
use store::DatabaseConnection;
use uuid::Uuid;

fn at(minutes: i64) -> Result<Timestamp> {
    Timestamp::from_millis(Duration::from_minutes(minutes).as_millis()).context("in range")
}

/// A stand-in for the token hash named `name`, as long as a real one. The column is `char(64)`, which Postgres pads
/// with spaces, so a shorter key would come back from a read different from how it went in.
fn hash(name: &str) -> String {
    format!("{name:-<64}")
}

struct Fixture {
    db: DatabaseConnection,
    user_id: UserId,
}

impl Fixture {
    async fn start() -> Result<Self> {
        let db = store::testing::fresh_database().await?;
        let user_id = Self::add_user(&db, "ada").await?;
        Ok(Self { db, user_id })
    }

    async fn add_user(db: &DatabaseConnection, username: &str) -> Result<UserId> {
        let user = User {
            id: UserId::from_uuid(Uuid::now_v7()),
            username: Handle::parse(username, Field::Username)?,
            email: Email::parse(&format!("{username}@example.com"))?,
            display_name: DisplayName::parse("Ada")?,
            created_at: at(0)?,
            version: Version::INITIAL,
        };
        store::users::insert_user(db, &user).await?;
        Ok(user.id)
    }

    /// A link of `user_id` named `name`, made at minute `made` and working for the usual hour.
    async fn link(&self, user_id: UserId, name: &str, made: i64) -> Result<String> {
        let created_at = at(made)?;
        let link = ResetLink {
            user_id,
            created_at,
            expires_at: created_at.checked_add(RESET_LINK_LIFETIME).context("in range")?,
            used_at: None,
            cancelled_at: None,
        };
        store::password_resets::insert_reset_link(&self.db, &hash(name), &link).await?;
        Ok(hash(name))
    }

    async fn state(&self, token_hash: &str, minute: i64) -> Result<ResetLinkState> {
        let link = store::password_resets::find_reset_link(&self.db, token_hash)
            .await?
            .context("link exists")?;
        Ok(link.state(at(minute)?))
    }
}

#[gtest]
#[tokio::test]
async fn a_link_is_stored_and_found_by_its_hash() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;
    let token_hash = fixture.link(fixture.user_id, "a", 10).await?;

    // Act
    let found = store::password_resets::find_reset_link(&fixture.db, &token_hash).await?;
    let unknown = store::password_resets::find_reset_link(&fixture.db, &hash("other")).await?;

    // Assert
    expect_that!(
        found,
        some(eq(ResetLink {
            user_id: fixture.user_id,
            created_at: at(10)?,
            expires_at: at(70)?,
            used_at: None,
            cancelled_at: None,
        }))
    );
    expect_that!(unknown, none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn cancelling_takes_only_the_working_links_of_the_user() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;
    let other_user = Fixture::add_user(&fixture.db, "sam").await?;
    let cancelled = fixture.link(fixture.user_id, "cancelled", 0).await?;
    store::password_resets::cancel_working_reset_links(&fixture.db, fixture.user_id, at(5)?).await?;
    let ended = fixture.link(fixture.user_id, "ended", 0).await?;
    let used = fixture.link(fixture.user_id, "used", 10).await?;
    store::password_resets::use_reset_link(&fixture.db, &used, at(20)?).await?;
    let working = fixture.link(fixture.user_id, "working", 10).await?;
    let theirs = fixture.link(other_user, "theirs", 10).await?;

    // Act
    let count = store::password_resets::cancel_working_reset_links(&fixture.db, fixture.user_id, at(65)?).await?;

    // Assert
    expect_that!(count, eq(1));
    expect_that!(fixture.state(&working, 65).await?, eq(ResetLinkState::Cancelled));
    expect_that!(fixture.state(&used, 65).await?, eq(ResetLinkState::Used));
    expect_that!(fixture.state(&cancelled, 65).await?, eq(ResetLinkState::Cancelled));
    expect_that!(fixture.state(&ended, 65).await?, eq(ResetLinkState::Ended));
    expect_that!(fixture.state(&theirs, 65).await?, eq(ResetLinkState::Working));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_is_used_once_and_only_while_it_works() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;
    let token_hash = fixture.link(fixture.user_id, "a", 0).await?;
    let cancelled = fixture.link(fixture.user_id, "b", 100).await?;
    store::password_resets::cancel_working_reset_links(&fixture.db, fixture.user_id, at(101)?).await?;
    let ended = fixture.link(fixture.user_id, "c", 200).await?;

    // Act
    let first = store::password_resets::use_reset_link(&fixture.db, &token_hash, at(10)?).await?;
    let second = store::password_resets::use_reset_link(&fixture.db, &token_hash, at(11)?).await?;
    let on_cancelled = store::password_resets::use_reset_link(&fixture.db, &cancelled, at(102)?).await?;
    let on_ended = store::password_resets::use_reset_link(&fixture.db, &ended, at(260)?).await?;
    let on_unknown = store::password_resets::use_reset_link(&fixture.db, &hash("none"), at(10)?).await?;

    // Assert
    expect_that!(
        (first, second, on_cancelled, on_ended, on_unknown),
        eq((true, false, false, false, false))
    );
    expect_that!(fixture.state(&token_hash, 12).await?, eq(ResetLinkState::Used));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_a_password_replaces_the_hash_or_inserts_one() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;

    // Act
    store::users::set_password(&fixture.db, fixture.user_id, "first", at(1)?).await?;
    let inserted = store::users::find_password_hash(&fixture.db, fixture.user_id).await?;
    store::users::set_password(&fixture.db, fixture.user_id, "second", at(2)?).await?;
    let replaced = store::users::find_password_hash(&fixture.db, fixture.user_id).await?;

    // Assert
    expect_that!(inserted, some(eq("first")));
    expect_that!(replaced, some(eq("second")));
    Ok(())
}
