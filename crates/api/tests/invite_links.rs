//! A team's invite links: making, listing and revoking them, who may, how long they work, and what the team's
//! activity says.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::Duration;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

const WEEK_MILLIS: i64 = 7 * 24 * 60 * 60 * 1000;

struct Setup {
    app: TestApp,
    ana: String,
    ana_id: String,
    team_id: String,
    team_url: String,
}

/// Ana, admin of a shared team of her own.
async fn setup() -> Result<Setup> {
    let app = TestApp::start().await?;
    let (ana, _) = app.register_as("ana@example.test", "Ana").await?;
    let ana_id = user_id(&app, &ana).await?;
    let created = app
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    let team_id = created.body["id"].as_str().context("team has no id")?.to_owned();
    let team_url = app.team_url(&team_id).await?;
    Ok(Setup {
        app,
        ana,
        ana_id,
        team_id,
        team_url,
    })
}

async fn user_id(app: &TestApp, session: &str) -> Result<String> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    Ok(me.body["user"]["id"].as_str().context("user has no id")?.to_owned())
}

/// Registers someone and adds them to the team with `role`.
async fn join(setup: &Setup, email: &str, name: &str, role: Role) -> Result<(String, String)> {
    let (session, _) = setup.app.register_as(email, name).await?;
    let id = user_id(&setup.app, &session).await?;
    setup.app.add_member(&setup.team_id, &id, role).await?;
    Ok((session, id))
}

fn links_url(setup: &Setup) -> String {
    format!("{}/invite-links", setup.team_url)
}

async fn make(setup: &Setup, session: &str) -> Result<TestResponse> {
    setup
        .app
        .send(Method::POST, &links_url(setup), Some(session), None)
        .await
}

async fn list(setup: &Setup, session: &str) -> Result<TestResponse> {
    setup
        .app
        .send(Method::GET, &links_url(setup), Some(session), None)
        .await
}

async fn revoke(setup: &Setup, session: &str, id: &str) -> Result<TestResponse> {
    setup
        .app
        .send(
            Method::POST,
            &format!("{}/{id}/revoke", links_url(setup)),
            Some(session),
            None,
        )
        .await
}

fn id_of(link: &Value) -> Result<String> {
    Ok(link["id"].as_str().context("link has no id")?.to_owned())
}

fn offers_invite_links(team: &Value) -> bool {
    team["allowed_actions"]
        .as_array()
        .is_some_and(|actions| actions.contains(&json!("manage_invite_links")))
}

async fn actions_for(setup: &Setup, link_id: &str) -> Result<Vec<String>> {
    Ok(setup
        .app
        .audit_entries(link_id)
        .await?
        .into_iter()
        .map(|(action, _)| action)
        .collect())
}

#[gtest]
#[tokio::test]
async fn an_admin_makes_a_working_link() -> Result<()> {
    // Arrange
    let setup = setup().await?;

    // Act
    let made = make(&setup, &setup.ana).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(made.body["state"], eq(&json!("working")));
    let token = made.body["token"].as_str().context("no token")?.to_owned();
    expect_that!(token.len(), eq(43));
    let created_at = made.body["created_at"].as_i64().context("no created_at")?;
    expect_that!(made.body["ends_at"], eq(&json!(created_at + WEEK_MILLIS)));
    expect_that!(made.body["ends_label"], eq(&json!("8 Jan")));
    expect_that!(made.body["created_by"]["id"], eq(&json!(setup.ana_id)));
    expect_that!(made.body["created_by"]["display_name"], eq(&json!("Ana")));
    expect_that!(made.body["allowed_actions"], eq(&json!(["revoke"])));
    expect_that!(made.body["revoked_by"], eq(&Value::Null));

    let id = id_of(&made.body)?;
    let entries = setup.app.audit_entries(&id).await?;
    expect_that!(entries.len(), eq(1));
    expect_that!(entries[0].0, eq("invite_link_created"));
    expect_that!(entries[0].1, eq(&json!({ "ends_at": created_at + WEEK_MILLIS })));
    expect_that!(entries[0].1.to_string(), not(contains_substring(token.as_str())));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_list_shows_every_link_newest_first_with_tokens_only_while_working() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let older = id_of(&make(&setup, &setup.ana).await?.body)?;
    setup.app.clock.advance(Duration::from_minutes(1));
    let newer = id_of(&make(&setup, &setup.ana).await?.body)?;
    setup.app.clock.advance(Duration::from_minutes(1));
    revoke(&setup, &setup.ana, &older).await?;

    // Act
    let listed = list(&setup, &setup.ana).await?;

    // Assert
    expect_that!(listed.status, eq(StatusCode::OK));
    let links = listed.body["links"].as_array().context("no links")?;
    expect_that!(links.len(), eq(2));
    expect_that!(links[0]["id"], eq(&json!(newer)));
    expect_that!(links[0]["state"], eq(&json!("working")));
    expect_that!(links[0]["token"].as_str().map(str::len), some(eq(43)));
    expect_that!(links[1]["id"], eq(&json!(older)));
    expect_that!(links[1]["state"], eq(&json!("revoked")));
    expect_that!(links[1]["token"], eq(&Value::Null));
    expect_that!(links[1]["revoked_by"]["id"], eq(&json!(setup.ana_id)));
    expect_that!(links[1]["revoked_at"].is_i64(), eq(true));
    expect_that!(links[1]["allowed_actions"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_ends_a_week_after_it_was_made() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let made = make(&setup, &setup.ana).await?;
    let token = made.body["token"].clone();

    // Act and assert
    setup.app.clock.advance(Duration::from_millis(WEEK_MILLIS - 1));
    let before = list(&setup, &setup.ana).await?;
    expect_that!(before.body["links"][0]["state"], eq(&json!("working")));
    expect_that!(before.body["links"][0]["token"], eq(&token));

    setup.app.clock.advance(Duration::from_millis(1));
    let after = list(&setup, &setup.ana).await?;
    expect_that!(after.body["links"][0]["state"], eq(&json!("ended")));
    expect_that!(after.body["links"][0]["token"], eq(&Value::Null));
    expect_that!(after.body["links"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(after.body["links"][0]["ends_label"], eq(&json!("today")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn revoking_stops_a_link_at_once_and_twice_changes_nothing() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;

    // Act
    let first = revoke(&setup, &setup.ana, &id).await?;
    let second = revoke(&setup, &setup.ana, &id).await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(first.body["state"], eq(&json!("revoked")));
    expect_that!(first.body["token"], eq(&Value::Null));
    expect_that!(first.body["revoked_by"]["id"], eq(&json!(setup.ana_id)));
    expect_that!(first.body["version"], eq(&json!(2)));
    expect_that!(second.status, eq(StatusCode::OK));
    expect_that!(second.body, eq(&first.body));
    expect_that!(
        actions_for(&setup, &id).await?,
        elements_are![eq("invite_link_created"), eq("invite_link_revoked")]
    );
    let entries = setup.app.audit_entries(&id).await?;
    expect_that!(
        entries[1].1,
        eq(&json!({ "state": { "before": "working", "after": "revoked" } }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn revoking_an_ended_link_changes_nothing() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;
    setup.app.clock.advance(Duration::from_millis(WEEK_MILLIS));

    // Act
    let revoked = revoke(&setup, &setup.ana, &id).await?;

    // Assert
    expect_that!(revoked.status, eq(StatusCode::OK));
    expect_that!(revoked.body["state"], eq(&json!("ended")));
    expect_that!(revoked.body["token"], eq(&Value::Null));
    expect_that!(revoked.body["revoked_by"], eq(&Value::Null));
    expect_that!(revoked.body["version"], eq(&json!(1)));
    expect_that!(
        actions_for(&setup, &id).await?,
        elements_are![eq("invite_link_created")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_link_outlives_its_makers_admin_role() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (ben, _) = join(&setup, "ben@example.test", "Ben", Role::Admin).await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;
    let demoted = setup
        .app
        .send(
            Method::PATCH,
            &setup.app.member_url(&setup.team_id, &setup.ana_id).await?,
            Some(&ben),
            Some(json!({ "role": "member" })),
        )
        .await?;
    anyhow::ensure!(demoted.status == StatusCode::OK, "demote failed: {}", demoted.body);

    // Act
    let ana_lists = list(&setup, &setup.ana).await?;
    let ana_revokes = revoke(&setup, &setup.ana, &id).await?;
    let ben_lists = list(&setup, &ben).await?;
    let ben_revokes = revoke(&setup, &ben, &id).await?;

    // Assert
    expect_that!(ana_lists.status, eq(StatusCode::FORBIDDEN));
    expect_that!(ana_revokes.status, eq(StatusCode::FORBIDDEN));
    expect_that!(ben_lists.status, eq(StatusCode::OK));
    expect_that!(ben_lists.body["links"][0]["state"], eq(&json!("working")));
    expect_that!(
        ben_lists.body["links"][0]["created_by"]["display_name"],
        eq(&json!("Ana"))
    );
    expect_that!(ben_revokes.status, eq(StatusCode::OK));
    expect_that!(ben_revokes.body["state"], eq(&json!("revoked")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_keeps_working_when_its_maker_leaves() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (ben, _) = join(&setup, "ben@example.test", "Ben", Role::Admin).await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;
    let left = setup
        .app
        .send(
            Method::POST,
            &format!("{}/leave", setup.team_url),
            Some(&setup.ana),
            None,
        )
        .await?;
    anyhow::ensure!(left.status == StatusCode::NO_CONTENT, "leave failed: {}", left.body);

    // Act
    let listed = list(&setup, &ben).await?;

    // Assert
    expect_that!(listed.body["links"][0]["id"], eq(&json!(id)));
    expect_that!(listed.body["links"][0]["state"], eq(&json!("working")));
    expect_that!(listed.body["links"][0]["created_by"]["display_name"], eq(&json!("Ana")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_personal_team_has_no_invite_links() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let me = setup
        .app
        .send(Method::GET, "/api/v1/me", Some(&setup.ana), None)
        .await?;
    let personal = me.body["teams"]
        .as_array()
        .context("no teams")?
        .iter()
        .find(|team| team["personal"] == json!(true))
        .context("no personal team")?;
    let url = format!(
        "/api/v1/teams/{}/invite-links",
        personal["slug"].as_str().context("no slug")?
    );

    // Act
    let made = setup.app.send(Method::POST, &url, Some(&setup.ana), None).await?;
    let listed = setup.app.send(Method::GET, &url, Some(&setup.ana), None).await?;
    let revoked = setup
        .app
        .send(
            Method::POST,
            &format!("{url}/7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40/revoke"),
            Some(&setup.ana),
            None,
        )
        .await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::CONFLICT));
    expect_that!(made.body["code"], eq(&json!("personal_team")));
    expect_that!(listed.status, eq(StatusCode::OK));
    expect_that!(listed.body, eq(&json!({ "links": [] })));
    expect_that!(revoked.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn another_teams_link_is_not_found() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let other = setup
        .app
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&setup.ana),
            Some(json!({ "name": "Sales" })),
        )
        .await?;
    let other_url = format!(
        "/api/v1/teams/{}/invite-links",
        other.body["slug"].as_str().context("no slug")?
    );
    let theirs = id_of(
        &setup
            .app
            .send(Method::POST, &other_url, Some(&setup.ana), None)
            .await?
            .body,
    )?;

    // Act
    let revoked = revoke(&setup, &setup.ana, &theirs).await?;
    let malformed = revoke(&setup, &setup.ana, "not-a-uuid").await?;

    // Assert
    expect_that!(revoked.status, eq(StatusCode::NOT_FOUND));
    expect_that!(malformed.status, eq(StatusCode::NOT_FOUND));
    expect_that!(
        actions_for(&setup, &theirs).await?,
        elements_are![eq("invite_link_created")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn admins_of_shared_teams_are_offered_invite_links() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (member, _) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;

    // Act
    let admin_view = setup
        .app
        .send(Method::GET, &setup.team_url, Some(&setup.ana), None)
        .await?;
    let member_view = setup
        .app
        .send(Method::GET, &setup.team_url, Some(&member), None)
        .await?;
    let me = setup
        .app
        .send(Method::GET, "/api/v1/me", Some(&setup.ana), None)
        .await?;

    // Assert
    expect_that!(offers_invite_links(&admin_view.body), eq(true));
    expect_that!(offers_invite_links(&member_view.body), eq(false));
    let personal = me.body["teams"]
        .as_array()
        .context("no teams")?
        .iter()
        .find(|team| team["personal"] == json!(true))
        .context("no personal team")?;
    expect_that!(offers_invite_links(personal), eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_team_activity_words_invite_links() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (member, _) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;
    setup.app.clock.advance(Duration::from_minutes(1));
    revoke(&setup, &setup.ana, &id).await?;
    let audit_url = format!("{}/audit", setup.team_url);

    // Act
    let admin_log = setup.app.send(Method::GET, &audit_url, Some(&setup.ana), None).await?;
    let member_log = setup.app.send(Method::GET, &audit_url, Some(&member), None).await?;

    // Assert
    let entries = admin_log.body["entries"].as_array().context("no entries")?;
    let revoked = &entries[0];
    expect_that!(revoked["action"], eq(&json!("invite_link_revoked")));
    expect_that!(revoked["actor"]["display_name"], eq(&json!("Ana")));
    expect_that!(revoked["phrase"], eq(&json!("revoked")));
    expect_that!(revoked["names_entity"], eq(&json!(true)));
    expect_that!(revoked["entity"]["type"], eq(&json!("invite_link")));
    expect_that!(revoked["entity"]["label"], eq(&json!("Ana's invite link")));
    let made = &entries[1];
    expect_that!(made["action"], eq(&json!("invite_link_created")));
    expect_that!(made["phrase"], eq(&json!("made an invite link")));
    expect_that!(made["names_entity"], eq(&json!(false)));

    let seen: Vec<&str> = member_log.body["entries"]
        .as_array()
        .context("no entries")?
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();
    expect_that!(seen, not(contains(eq(&"invite_link_created"))));
    expect_that!(seen, not(contains(eq(&"invite_link_revoked"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_are_refused_and_outsiders_find_nothing() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (member, _) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;
    let (outsider, _) = setup.app.register_as("zed@example.test", "Zed").await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;

    // Act and assert
    expect_that!(make(&setup, &member).await?.status, eq(StatusCode::FORBIDDEN));
    expect_that!(list(&setup, &member).await?.status, eq(StatusCode::FORBIDDEN));
    expect_that!(revoke(&setup, &member, &id).await?.status, eq(StatusCode::FORBIDDEN));
    expect_that!(make(&setup, &outsider).await?.status, eq(StatusCode::NOT_FOUND));
    expect_that!(list(&setup, &outsider).await?.status, eq(StatusCode::NOT_FOUND));
    expect_that!(revoke(&setup, &outsider, &id).await?.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

/// Two admins revoking the same link at the same moment: one revokes, the other finds it revoked, and the audit log
/// has one revocation. Both the team lock and the conditional update stand behind this.
#[gtest]
#[tokio::test]
async fn two_admins_revoking_at_once_record_one_revocation() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (ben, _) = join(&setup, "ben@example.test", "Ben", Role::Admin).await?;
    let id = id_of(&make(&setup, &setup.ana).await?.body)?;

    // Act
    let (by_ana, by_ben) = tokio::join!(revoke(&setup, &setup.ana, &id), revoke(&setup, &ben, &id));
    let (by_ana, by_ben) = (by_ana?, by_ben?);

    // Assert
    expect_that!(by_ana.status, eq(StatusCode::OK));
    expect_that!(by_ben.status, eq(StatusCode::OK));
    expect_that!(by_ana.body, eq(&by_ben.body));
    expect_that!(by_ana.body["state"], eq(&json!("revoked")));
    expect_that!(
        actions_for(&setup, &id).await?,
        elements_are![eq("invite_link_created"), eq("invite_link_revoked")]
    );
    Ok(())
}
