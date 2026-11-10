//! Invite link storage rules that the service's own checks would otherwise hide.
//!
//! Runs on in-memory SQLite, or on MySQL or Postgres with `TEST_DATABASE_URL` set; the MySQL run in CI is what proves
//! the `utf8mb4_bin` token column.

use anyhow::{Context, Result};
use domain::handle::Handle;
use domain::invite_link::{InviteLink, InviteToken};
use domain::team::Team;
use domain::text::Name;
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{InviteLinkId, TeamId, Timestamp, UserId, Version};
use googletest::prelude::*;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

/// A user and a shared team in `db`, to hang links on.
async fn seed(db: &DatabaseConnection) -> Result<(User, Team)> {
    let now = Timestamp::from_millis(0).context("in range")?;
    let user = User {
        id: UserId::from_uuid(Uuid::now_v7()),
        username: Handle::parse("ada", Field::Username)?,
        email: Email::parse("ada@example.com")?,
        display_name: DisplayName::parse("Ada")?,
        created_at: now,
        version: Version::INITIAL,
    };
    store::users::insert_user(db, &user).await?;
    let team = Team {
        id: TeamId::from_uuid(Uuid::now_v7()),
        slug: Handle::parse("team", Field::Slug)?,
        name: Name::parse("Team")?,
        personal_user: None,
        created_at: now,
        version: Version::INITIAL,
    };
    store::teams::insert_team(db, &team).await?;
    Ok((user, team))
}

fn link_with(user: &User, team: &Team, token: &str) -> InviteLink {
    InviteLink {
        id: InviteLinkId::from_uuid(Uuid::now_v7()),
        team_id: team.id,
        token: InviteToken::new(token.to_owned()),
        created_by: user.id,
        created_at: team.created_at,
        revocation: None,
        version: Version::INITIAL,
    }
}

/// Tokens are base64url, so two that differ only in case are different tokens: both are stored, and each is read
/// back exactly as it was made.
#[gtest]
#[tokio::test]
async fn tokens_differing_only_in_case_are_distinct() -> Result<()> {
    let db = store::testing::fresh_database().await?;
    let (user, team) = seed(&db).await?;
    let lower = "a".repeat(43);
    let upper = "A".repeat(43);

    store::invite_links::insert_invite_link(&db, &link_with(&user, &team, &lower)).await?;
    store::invite_links::insert_invite_link(&db, &link_with(&user, &team, &upper)).await?;

    let tokens: Vec<String> = store::invite_links::list_invite_links(&db, team.id)
        .await?
        .iter()
        .map(|link| link.token.expose().to_owned())
        .collect();
    expect_that!(tokens, unordered_elements_are![eq(&lower), eq(&upper)]);
    Ok(())
}

/// Finding by token is an exact match: a token that differs only in case, or that nobody made, finds nothing.
#[gtest]
#[tokio::test]
async fn a_link_is_found_by_its_exact_token() -> Result<()> {
    let db = store::testing::fresh_database().await?;
    let (user, team) = seed(&db).await?;
    let token = format!("a{}", "B".repeat(42));
    let made = link_with(&user, &team, &token);
    store::invite_links::insert_invite_link(&db, &made).await?;

    let found = store::invite_links::find_invite_link_by_token(&db, &InviteToken::new(token.clone())).await?;
    let other_case =
        store::invite_links::find_invite_link_by_token(&db, &InviteToken::new(token.to_uppercase())).await?;
    let unknown = store::invite_links::find_invite_link_by_token(&db, &InviteToken::new("z".repeat(43))).await?;

    expect_that!(found, some(eq(&made)));
    expect_that!(other_case, none());
    expect_that!(unknown, none());
    Ok(())
}
