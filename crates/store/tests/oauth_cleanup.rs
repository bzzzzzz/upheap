//! Removing OAuth codes, token families and tokens that ended. Runs on in-memory SQLite, or on MySQL or Postgres with
//! `TEST_DATABASE_URL` set; those runs prove the key-then-delete reads and the foreign keys on each backend.

use anyhow::{Context, Result};
use domain::handle::Handle;
use domain::oauth::{CodeChallenge, TokenKind};
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{Duration, Timestamp, TokenFamilyId, UserId, Version};
use googletest::prelude::*;
use store::oauth::{NewAuthCode, NewToken};
use store::{ConnectionTrait, DatabaseConnection, Statement};
use uuid::Uuid;

fn at(hours: i64) -> Result<Timestamp> {
    Timestamp::from_millis(Duration::from_hours(hours).as_millis()).context("in range")
}

/// A stand-in for the hash named `name`, as long as a real one. The columns are `char(64)`, which Postgres pads with
/// spaces, so a shorter key would come back from a read different from how it went in.
fn hash(name: &str) -> String {
    format!("{name:-<64}")
}

/// The time the fixtures call "now" when they are made, long before every cutoff the tests use.
const MADE_AT: i64 = 0;
/// The cutoff most tests remove at: rows ended by hour 100 go.
const CUTOFF: i64 = 100;

struct Fixture {
    db: DatabaseConnection,
    user_id: UserId,
}

impl Fixture {
    async fn start() -> Result<Self> {
        let db = store::testing::fresh_database().await?;
        let user = User {
            id: UserId::from_uuid(Uuid::now_v7()),
            username: Handle::parse("ada", Field::Username)?,
            email: Email::parse("ada@example.com")?,
            display_name: DisplayName::parse("Ada")?,
            created_at: at(MADE_AT)?,
            version: Version::INITIAL,
        };
        store::users::insert_user(&db, &user).await?;
        Ok(Self { db, user_id: user.id })
    }

    /// A family made at hour 0, revoked at `revoked_at` hours when given.
    async fn family(&self, revoked_at: Option<i64>) -> Result<TokenFamilyId> {
        let id = TokenFamilyId::from_uuid(Uuid::now_v7());
        store::oauth::insert_token_family(&self.db, id, self.user_id, "upheap-cli", None, at(MADE_AT)?).await?;
        if let Some(hours) = revoked_at {
            store::oauth::revoke_token_family(&self.db, id, at(hours)?).await?;
        }
        Ok(id)
    }

    /// A token of `family` that expires at `expires_at` hours, and is marked used when `used` is set.
    async fn token(
        &self,
        name: &str,
        family: TokenFamilyId,
        kind: TokenKind,
        expires_at: i64,
        used: bool,
    ) -> Result<()> {
        let hash = hash(name);
        store::oauth::insert_token(
            &self.db,
            &NewToken {
                token_hash: &hash,
                family_id: family,
                kind,
                created_at: at(MADE_AT)?,
                expires_at: at(expires_at)?,
            },
        )
        .await?;
        if used {
            store::oauth::use_refresh_token(&self.db, &hash, at(MADE_AT)?).await?;
        }
        Ok(())
    }

    /// An access and a refresh token for `family`, both expiring at `expires_at` hours.
    async fn pair(&self, name: &str, family: TokenFamilyId, expires_at: i64) -> Result<()> {
        self.token(&format!("{name}-access"), family, TokenKind::Access, expires_at, false)
            .await?;
        self.token(
            &format!("{name}-refresh"),
            family,
            TokenKind::Refresh,
            expires_at,
            false,
        )
        .await
    }

    /// A code expiring at `expires_at` hours, exchanged for `family` when one is given.
    async fn code(&self, name: &str, expires_at: i64, family: Option<TokenFamilyId>) -> Result<()> {
        let hash = hash(name);
        let challenge = CodeChallenge::from_stored("c".repeat(43));
        store::oauth::insert_auth_code(
            &self.db,
            &NewAuthCode {
                code_hash: &hash,
                user_id: self.user_id,
                client_id: "upheap-cli",
                redirect_uri: "http://127.0.0.1:1/callback",
                code_challenge: &challenge,
                created_at: at(MADE_AT)?,
                expires_at: at(expires_at)?,
            },
        )
        .await?;
        if let Some(family) = family {
            let used = store::oauth::use_auth_code(&self.db, &hash, at(MADE_AT)?, family).await?;
            anyhow::ensure!(used, "code {name} was not marked used");
        }
        Ok(())
    }

    async fn count(&self, table: &str) -> Result<i64> {
        let sql = format!("SELECT COUNT(*) AS n FROM {table}");
        let row = self
            .db
            .query_one_raw(Statement::from_string(self.db.get_database_backend(), sql))
            .await?
            .context("no count row")?;
        Ok(row.try_get("", "n")?)
    }

    async fn has_code(&self, name: &str) -> Result<bool> {
        Ok(store::oauth::find_auth_code(&self.db, &hash(name)).await?.is_some())
    }

    async fn has_family(&self, id: TokenFamilyId) -> Result<bool> {
        Ok(store::oauth::find_token_family(&self.db, id).await?.is_some())
    }

    async fn has_token(&self, name: &str) -> Result<bool> {
        Ok(store::oauth::find_token(&self.db, &hash(name)).await?.is_some())
    }
}

#[gtest]
#[tokio::test]
async fn ended_codes_are_removed_and_recent_ones_kept() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;
    let used_by = fixture.family(None).await?;
    fixture.pair("used-by", used_by, CUTOFF + 50).await?;
    fixture.code("unused-ended", 50, None).await?;
    fixture.code("used-ended", 60, Some(used_by)).await?;
    fixture.code("expired-after-cutoff", CUTOFF + 1, None).await?;
    fixture.code("unexpired", CUTOFF + 50, None).await?;

    // Act
    let removed = store::oauth::delete_ended_auth_codes(&fixture.db, at(CUTOFF)?, 10).await?;

    // Assert
    expect_that!(removed, eq(2));
    expect_that!(fixture.has_code("unused-ended").await?, eq(false));
    expect_that!(fixture.has_code("used-ended").await?, eq(false));
    expect_that!(fixture.has_code("expired-after-cutoff").await?, eq(true));
    expect_that!(fixture.has_code("unexpired").await?, eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn ended_families_are_removed_with_their_tokens() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;
    let revoked = fixture.family(Some(50)).await?;
    fixture.pair("revoked", revoked, CUTOFF + 50).await?;
    let expired = fixture.family(None).await?;
    fixture.pair("expired", expired, 50).await?;
    let refreshed = fixture.family(None).await?;
    fixture
        .token(
            "refreshed-first-access",
            refreshed,
            TokenKind::Access,
            CUTOFF + 50,
            false,
        )
        .await?;
    fixture
        .token(
            "refreshed-first-refresh",
            refreshed,
            TokenKind::Refresh,
            CUTOFF + 50,
            true,
        )
        .await?;
    fixture.pair("refreshed-next", refreshed, CUTOFF + 60).await?;
    let revoked_lately = fixture.family(Some(CUTOFF + 1)).await?;
    fixture.pair("revoked-lately", revoked_lately, CUTOFF + 50).await?;
    let live = fixture.family(None).await?;
    fixture.pair("live", live, CUTOFF + 50).await?;
    // Its only refresh token expired after the cutoff, so the family has been unusable for less than a day.
    let spent_lately = fixture.family(None).await?;
    fixture.pair("spent-lately", spent_lately, CUTOFF + 1).await?;

    // Act
    let removed = store::oauth::delete_ended_token_families(&fixture.db, at(CUTOFF)?, 10).await?;

    // Assert
    expect_that!(removed, eq(2));
    expect_that!(fixture.has_family(revoked).await?, eq(false));
    expect_that!(fixture.has_token("revoked-access").await?, eq(false));
    expect_that!(fixture.has_token("revoked-refresh").await?, eq(false));
    expect_that!(fixture.has_family(expired).await?, eq(false));
    expect_that!(fixture.has_token("expired-access").await?, eq(false));
    expect_that!(fixture.has_token("expired-refresh").await?, eq(false));
    for kept in [refreshed, revoked_lately, live, spent_lately] {
        expect_that!(fixture.has_family(kept).await?, eq(true));
    }
    // Four kept families hold a pair each, and the refreshed one two pairs.
    expect_that!(fixture.count("oauth_tokens").await?, eq(10));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_family_a_code_still_names_is_kept() -> Result<()> {
    // Arrange: revoked long before the cutoff, but the code that made it hasn't ended.
    let fixture = Fixture::start().await?;
    let family = fixture.family(Some(10)).await?;
    fixture.pair("named", family, 20).await?;
    fixture.code("names-family", CUTOFF + 50, Some(family)).await?;

    // Act
    let removed_early = store::oauth::delete_ended_token_families(&fixture.db, at(CUTOFF)?, 10).await?;
    let codes_removed = store::oauth::delete_ended_auth_codes(&fixture.db, at(CUTOFF + 100)?, 10).await?;
    let removed_later = store::oauth::delete_ended_token_families(&fixture.db, at(CUTOFF + 100)?, 10).await?;

    // Assert
    expect_that!(removed_early, eq(0));
    expect_that!(codes_removed, eq(1));
    expect_that!(removed_later, eq(1));
    expect_that!(fixture.count("oauth_token_families").await?, eq(0));
    expect_that!(fixture.count("oauth_tokens").await?, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn removal_takes_at_most_a_batch() -> Result<()> {
    // Arrange
    let fixture = Fixture::start().await?;
    for name in ["a", "b", "c"] {
        fixture.code(&format!("code-{name}"), 50, None).await?;
        let family = fixture.family(Some(50)).await?;
        fixture.pair(&format!("family-{name}"), family, 50).await?;
    }
    let cutoff = at(CUTOFF)?;

    // Act
    let codes = [
        store::oauth::delete_ended_auth_codes(&fixture.db, cutoff, 2).await?,
        store::oauth::delete_ended_auth_codes(&fixture.db, cutoff, 2).await?,
        store::oauth::delete_ended_auth_codes(&fixture.db, cutoff, 2).await?,
    ];
    let families = [
        store::oauth::delete_ended_token_families(&fixture.db, cutoff, 2).await?,
        store::oauth::delete_ended_token_families(&fixture.db, cutoff, 2).await?,
        store::oauth::delete_ended_token_families(&fixture.db, cutoff, 2).await?,
    ];

    // Assert
    expect_that!(codes, eq([2, 1, 0]));
    expect_that!(families, eq([2, 1, 0]));
    expect_that!(fixture.count("oauth_tokens").await?, eq(0));
    Ok(())
}
