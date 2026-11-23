//! A person's default heap (`PUT` and `DELETE …/default`): setting, moving and clearing it, `default_heap` on
//! `GET /me`, and its audit entry in the personal team.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};

/// A signed-in user with two heaps in one team: `first` and `second`.
struct Setup {
    app: TestApp,
    session: String,
    team: String,
    first: String,
    second: String,
}

async fn setup() -> Result<Setup> {
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, first) = app.shared_heap(&session).await?;
    let team_url = app.team_url(&team).await?;
    let made = app
        .send(
            Method::POST,
            &format!("{team_url}/heaps"),
            Some(&session),
            Some(json!({ "name": "Second" })),
        )
        .await?;
    let second = made.body["id"].as_str().context("no heap id")?.to_owned();
    Ok(Setup {
        app,
        session,
        team,
        first,
        second,
    })
}

impl Setup {
    async fn me(&self) -> Result<Value> {
        let me = self
            .app
            .send(Method::GET, "/api/v1/me", Some(&self.session), None)
            .await?;
        Ok(me.body)
    }

    async fn user_id(&self) -> Result<String> {
        Ok(self.me().await?["user"]["id"].as_str().context("no user")?.to_owned())
    }

    /// The `default_heap_changed` entries in the user's personal team, oldest first.
    async fn entries(&self) -> Result<Vec<Value>> {
        Ok(self
            .app
            .audit_entries(&self.user_id().await?)
            .await?
            .into_iter()
            .filter(|(action, _)| action == "default_heap_changed")
            .map(|(_, diff)| diff)
            .collect())
    }
}

#[gtest]
#[tokio::test]
async fn setting_a_default_shows_on_me_and_repeating_it_writes_no_second_entry() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let fresh = s.me().await?;

    // Act
    let set = s.app.set_default(&s.session, &s.first).await?;
    let again = s.app.set_default(&s.session, &s.first).await?;

    // Assert
    expect_that!(fresh["default_heap"], eq(&json!(null)));
    expect_that!(set.status, eq(StatusCode::NO_CONTENT));
    expect_that!(again.status, eq(StatusCode::NO_CONTENT));
    expect_that!(s.me().await?["default_heap"], eq(&json!(s.first)));
    expect_that!(
        s.entries().await?,
        elements_are![eq(&json!({ "heap_id": { "before": null, "after": s.first } }))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_another_heap_replaces_the_default_and_writes_a_second_entry() -> Result<()> {
    // Arrange
    let s = setup().await?;
    s.app.set_default(&s.session, &s.first).await?;

    // Act
    let moved = s.app.set_default(&s.session, &s.second).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::NO_CONTENT));
    expect_that!(s.me().await?["default_heap"], eq(&json!(s.second)));
    expect_that!(
        s.entries().await?,
        elements_are![
            eq(&json!({ "heap_id": { "before": null, "after": s.first } })),
            eq(&json!({ "heap_id": { "before": s.first, "after": s.second } })),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn clearing_the_default_empties_it_and_clearing_another_heap_changes_nothing() -> Result<()> {
    // Arrange
    let s = setup().await?;
    s.app.set_default(&s.session, &s.first).await?;

    // Act
    let other = s.app.clear_default(&s.session, &s.second).await?;
    let after_other = s.me().await?["default_heap"].clone();
    let entries_after_other = s.entries().await?.len();
    let cleared = s.app.clear_default(&s.session, &s.first).await?;
    let none_left = s.app.clear_default(&s.session, &s.first).await?;

    // Assert
    expect_that!(other.status, eq(StatusCode::NO_CONTENT));
    expect_that!(after_other, eq(&json!(s.first)));
    expect_that!(entries_after_other, eq(1));
    expect_that!(cleared.status, eq(StatusCode::NO_CONTENT));
    expect_that!(none_left.status, eq(StatusCode::NO_CONTENT));
    expect_that!(s.me().await?["default_heap"], eq(&json!(null)));
    expect_that!(
        s.entries().await?,
        elements_are![
            eq(&json!({ "heap_id": { "before": null, "after": s.first } })),
            eq(&json!({ "heap_id": { "before": s.first, "after": null } })),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_entry_is_in_the_personal_teams_log_and_not_the_heaps_team() -> Result<()> {
    // Arrange
    let s = setup().await?;

    // Act
    s.app.set_default(&s.session, &s.first).await?;

    // Assert
    expect_that!(s.entries().await?.len(), eq(1));
    let team_log = s.app.audit_entries(&s.team).await?;
    expect_that!(
        team_log.iter().any(|(action, _)| action == "default_heap_changed"),
        eq(false)
    );
    Ok(())
}
