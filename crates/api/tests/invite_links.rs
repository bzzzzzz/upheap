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

// Opening a link and joining through it.

/// A new working link of the team, made by Ana, and its token.
async fn new_token(setup: &Setup) -> Result<String> {
    let made = make(setup, &setup.ana).await?;
    Ok(made.body["token"].as_str().context("link has no token")?.to_owned())
}

async fn open(setup: &Setup, token: &str, session: Option<&str>) -> Result<TestResponse> {
    setup
        .app
        .send(Method::GET, &format!("/api/v1/invites/{token}"), session, None)
        .await
}

async fn join_through(setup: &Setup, token: &str, session: Option<&str>) -> Result<TestResponse> {
    setup
        .app
        .send(Method::POST, &format!("/api/v1/invites/{token}/join"), session, None)
        .await
}

/// Registers someone who is in no shared team.
async fn newcomer(setup: &Setup, email: &str, name: &str) -> Result<(String, String)> {
    let (session, _) = setup.app.register_as(email, name).await?;
    let id = user_id(&setup.app, &session).await?;
    Ok((session, id))
}

async fn remove(setup: &Setup, member_id: &str) -> Result<()> {
    let url = setup.app.member_url(&setup.team_id, member_id).await?;
    let removed = setup.app.send(Method::DELETE, &url, Some(&setup.ana), None).await?;
    anyhow::ensure!(
        removed.status == StatusCode::NO_CONTENT,
        "remove failed: {}",
        removed.body
    );
    Ok(())
}

async fn leave(setup: &Setup, session: &str) -> Result<()> {
    let url = format!("{}/leave", setup.team_url);
    let left = setup.app.send(Method::POST, &url, Some(session), None).await?;
    anyhow::ensure!(left.status == StatusCode::NO_CONTENT, "leave failed: {}", left.body);
    Ok(())
}

async fn team_audit(setup: &Setup, session: &str, query: &str) -> Result<Vec<Value>> {
    let url = format!("{}/audit{query}", setup.team_url);
    let log = setup.app.send(Method::GET, &url, Some(session), None).await?;
    Ok(log.body["entries"].as_array().context("no entries")?.clone())
}

fn role_of(team: &Value, user_id: &str) -> Option<String> {
    team["members"]
        .as_array()?
        .iter()
        .find(|member| member["user_id"] == json!(user_id))
        .and_then(|member| member["role"].as_str().map(str::to_owned))
}

#[gtest]
#[tokio::test]
async fn anyone_holding_a_link_sees_the_team_and_maker_names_and_nothing_else() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;

    // Act
    let opened = open(&setup, &token, None).await?;

    // Assert
    expect_that!(opened.status, eq(StatusCode::OK));
    expect_that!(
        opened.body,
        eq(&json!({ "team_name": "Ops", "created_by_name": "Ana", "state": "working", "viewer": null }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_or_malformed_token_is_not_found() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (sam, _) = newcomer(&setup, "sam@example.test", "Sam").await?;

    // Act
    let unknown = open(&setup, &"a".repeat(43), None).await?;
    let short = open(&setup, "short", None).await?;
    let punctuation = open(&setup, &format!("{}!", "a".repeat(42)), None).await?;
    let join_unknown = join_through(&setup, &"a".repeat(43), Some(&sam)).await?;
    let join_short = join_through(&setup, "short", Some(&sam)).await?;

    // Assert
    for response in [&unknown, &short, &punctuation, &join_unknown, &join_short] {
        expect_that!(response.status, eq(StatusCode::NOT_FOUND));
        expect_that!(response.body["code"], eq(&json!("not_found")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_ended_or_revoked_link_still_opens_and_says_which() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let ended = make(&setup, &setup.ana).await?;
    let ended_token = ended.body["token"].as_str().context("no token")?.to_owned();
    setup.app.clock.advance(Duration::from_days(7));
    let revoked = make(&setup, &setup.ana).await?;
    let revoked_token = revoked.body["token"].as_str().context("no token")?.to_owned();
    revoke(&setup, &setup.ana, &id_of(&revoked.body)?).await?;

    // Act
    let after_a_week = open(&setup, &ended_token, None).await?;
    let after_revoking = open(&setup, &revoked_token, None).await?;

    // Assert
    expect_that!(after_a_week.status, eq(StatusCode::OK));
    expect_that!(after_a_week.body["state"], eq(&json!("ended")));
    expect_that!(after_a_week.body["team_name"], eq(&json!("Ops")));
    expect_that!(after_a_week.body["created_by_name"], eq(&json!("Ana")));
    expect_that!(after_revoking.body["state"], eq(&json!("revoked")));
    expect_that!(after_revoking.body["team_name"], eq(&json!("Ops")));
    expect_that!(after_revoking.body["created_by_name"], eq(&json!("Ana")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_signed_in_viewer_is_told_where_they_stand() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (sam, _) = newcomer(&setup, "sam@example.test", "Sam").await?;
    let (mia, _) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;

    // Act
    let outsider = open(&setup, &token, Some(&sam)).await?;
    let member = open(&setup, &token, Some(&mia)).await?;
    let admin = open(&setup, &token, Some(&setup.ana)).await?;

    // Assert
    expect_that!(
        outsider.body["viewer"],
        eq(&json!({ "status": "not_member", "team_slug": null }))
    );
    expect_that!(
        member.body["viewer"],
        eq(&json!({ "status": "member", "team_slug": "ops" }))
    );
    expect_that!(
        admin.body["viewer"],
        eq(&json!({ "status": "member", "team_slug": "ops" }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn joining_makes_a_member_and_is_in_the_team_activity() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (mia, _) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;
    let (sam, sam_id) = newcomer(&setup, "sam@example.test", "Sam").await?;
    let link_id = id_of(&list(&setup, &setup.ana).await?.body["links"][0])?;
    setup.app.clock.advance(Duration::from_minutes(1));

    // Act
    let joined = join_through(&setup, &token, Some(&sam)).await?;
    let team = setup.app.send(Method::GET, &setup.team_url, Some(&sam), None).await?;

    // Assert
    expect_that!(joined.status, eq(StatusCode::CREATED));
    expect_that!(joined.body["name"], eq(&json!("Ops")));
    expect_that!(joined.body["role"], eq(&json!("member")));
    expect_that!(role_of(&joined.body, &sam_id), some(eq("member")));
    expect_that!(team.status, eq(StatusCode::OK));

    let entries = team_audit(&setup, &setup.ana, "").await?;
    let entry = &entries[0];
    expect_that!(entry["action"], eq(&json!("member_joined")));
    expect_that!(entry["actor"]["display_name"], eq(&json!("Sam")));
    expect_that!(entry["phrase"], eq(&json!("joined through")));
    expect_that!(entry["names_entity"], eq(&json!(true)));
    expect_that!(entry["entity"]["type"], eq(&json!("invite_link")));
    expect_that!(entry["entity"]["label"], eq(&json!("Ana's invite link")));
    expect_that!(entry["changes"], eq(&json!([])));

    let history = team_audit(&setup, &setup.ana, &format!("?entity_id={link_id}")).await?;
    let actions: Vec<&str> = history.iter().filter_map(|e| e["action"].as_str()).collect();
    expect_that!(actions, elements_are![eq(&"member_joined"), eq(&"invite_link_created")]);

    let seen_by_member: Vec<Value> = team_audit(&setup, &mia, "").await?;
    let member_actions: Vec<&str> = seen_by_member.iter().filter_map(|e| e["action"].as_str()).collect();
    expect_that!(member_actions, not(contains(eq(&"member_joined"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_keeps_working_for_the_next_person() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (sam, _) = newcomer(&setup, "sam@example.test", "Sam").await?;
    let (zoe, _) = newcomer(&setup, "zoe@example.test", "Zoe").await?;

    // Act
    let first = join_through(&setup, &token, Some(&sam)).await?;
    let second = join_through(&setup, &token, Some(&zoe)).await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::CREATED));
    expect_that!(second.status, eq(StatusCode::CREATED));
    expect_that!(second.body["members"].as_array().map(Vec::len), some(eq(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn someone_already_in_the_team_is_left_as_they_are() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (mia, mia_id) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;
    let before = team_audit(&setup, &setup.ana, "").await?.len();

    // Act
    let admin = join_through(&setup, &token, Some(&setup.ana)).await?;
    let member = join_through(&setup, &token, Some(&mia)).await?;

    // Assert
    expect_that!(admin.status, eq(StatusCode::OK));
    expect_that!(role_of(&admin.body, &setup.ana_id), some(eq("admin")));
    expect_that!(member.status, eq(StatusCode::OK));
    expect_that!(role_of(&member.body, &mia_id), some(eq("member")));
    expect_that!(team_audit(&setup, &setup.ana, "").await?.len(), eq(before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_stays_whatever_state_the_link_is_in() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (mia, _) = join(&setup, "mia@example.test", "Mia", Role::Member).await?;
    setup.app.clock.advance(Duration::from_days(8));

    // Act
    let opened = open(&setup, &token, Some(&mia)).await?;
    let joined = join_through(&setup, &token, Some(&mia)).await?;

    // Assert
    expect_that!(opened.body["state"], eq(&json!("ended")));
    expect_that!(opened.body["viewer"]["status"], eq(&json!("member")));
    expect_that!(opened.body["viewer"]["team_slug"], eq(&json!("ops")));
    expect_that!(joined.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_that_ended_or_was_revoked_since_it_was_opened_admits_nobody() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let ending = new_token(&setup).await?;
    let revoking = make(&setup, &setup.ana).await?;
    let revoking_token = revoking.body["token"].as_str().context("no token")?.to_owned();
    let (sam, sam_id) = newcomer(&setup, "sam@example.test", "Sam").await?;
    let opened = open(&setup, &revoking_token, Some(&sam)).await?;
    anyhow::ensure!(opened.body["state"] == json!("working"), "link should work");
    revoke(&setup, &setup.ana, &id_of(&revoking.body)?).await?;
    let before = team_audit(&setup, &setup.ana, "").await?.len();

    // Act
    let revoked = join_through(&setup, &revoking_token, Some(&sam)).await?;
    setup.app.clock.advance(Duration::from_days(7));
    let ended = join_through(&setup, &ending, Some(&sam)).await?;

    // Assert
    expect_that!(revoked.status, eq(StatusCode::CONFLICT));
    expect_that!(revoked.body["code"], eq(&json!("invite_link_revoked")));
    expect_that!(ended.status, eq(StatusCode::CONFLICT));
    expect_that!(ended.body["code"], eq(&json!("invite_link_ended")));
    let team_after = setup
        .app
        .send(Method::GET, &setup.team_url, Some(&setup.ana), None)
        .await?;
    expect_that!(role_of(&team_after.body, &sam_id), none());
    expect_that!(team_audit(&setup, &setup.ana, "").await?.len(), eq(before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_person_removed_after_the_link_was_made_cannot_join_through_it() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let monday = new_token(&setup).await?;
    let (sam, sam_id) = join(&setup, "sam@example.test", "Sam", Role::Member).await?;
    setup.app.clock.advance(Duration::from_days(2));
    remove(&setup, &sam_id).await?;
    setup.app.clock.advance(Duration::from_days(1));

    // Act
    let opened = open(&setup, &monday, Some(&sam)).await?;
    let refused = join_through(&setup, &monday, Some(&sam)).await?;
    let thursday = new_token(&setup).await?;
    let admitted = join_through(&setup, &thursday, Some(&sam)).await?;

    // Assert
    expect_that!(
        opened.body["viewer"],
        eq(&json!({ "status": "removed", "team_slug": null }))
    );
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(refused.body["code"], eq(&json!("removed_from_team")));
    expect_that!(admitted.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_person_who_left_by_choice_can_join_through_an_older_link() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (sam, _) = newcomer(&setup, "sam@example.test", "Sam").await?;
    join_through(&setup, &token, Some(&sam)).await?;
    setup.app.clock.advance(Duration::from_days(1));
    leave(&setup, &sam).await?;

    // Act
    let opened = open(&setup, &token, Some(&sam)).await?;
    let rejoined = join_through(&setup, &token, Some(&sam)).await?;

    // Assert
    expect_that!(opened.body["viewer"]["status"], eq(&json!("not_member")));
    expect_that!(rejoined.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_still_works_after_its_maker_is_demoted() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    join(&setup, "ben@example.test", "Ben", Role::Admin).await?;
    let (sam, _) = newcomer(&setup, "sam@example.test", "Sam").await?;
    let demoted = setup
        .app
        .send(
            Method::PATCH,
            &setup.app.member_url(&setup.team_id, &setup.ana_id).await?,
            Some(&setup.ana),
            Some(json!({ "role": "member" })),
        )
        .await?;
    anyhow::ensure!(demoted.status == StatusCode::OK, "demote failed: {}", demoted.body);

    // Act
    let opened = open(&setup, &token, None).await?;
    let joined = join_through(&setup, &token, Some(&sam)).await?;

    // Assert
    expect_that!(opened.body["created_by_name"], eq(&json!("Ana")));
    expect_that!(joined.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_direct_add_of_a_removed_person_still_works() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let (_, sam_id) = join(&setup, "sam@example.test", "Sam", Role::Member).await?;
    remove(&setup, &sam_id).await?;

    // Act
    let added = setup
        .app
        .send(
            Method::POST,
            &format!("{}/members", setup.team_url),
            Some(&setup.ana),
            Some(json!({ "user_id": sam_id })),
        )
        .await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::CREATED));
    expect_that!(role_of(&added.body, &sam_id), some(eq("member")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn joining_needs_a_browser_session_and_opening_takes_either() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (sam, sam_id) = newcomer(&setup, "sam@example.test", "Sam").await?;
    let (access, _) = setup.app.sign_in_cli(&sam).await?;

    // Act
    let signed_out = join_through(&setup, &token, None).await?;
    let with_bearer = setup
        .app
        .send_bearer(Method::POST, &format!("/api/v1/invites/{token}/join"), &access, None)
        .await?;
    let opened_with_bearer = setup
        .app
        .send_bearer(Method::GET, &format!("/api/v1/invites/{token}"), &access, None)
        .await?;
    let opened_with_bad_bearer = setup
        .app
        .send_bearer(Method::GET, &format!("/api/v1/invites/{token}"), "not-a-token", None)
        .await?;

    // Assert
    expect_that!(signed_out.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(signed_out.body["code"], eq(&json!("unauthenticated")));
    expect_that!(with_bearer.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(with_bearer.body["code"], eq(&json!("unauthenticated")));
    expect_that!(opened_with_bearer.status, eq(StatusCode::OK));
    expect_that!(opened_with_bearer.body["viewer"]["status"], eq(&json!("not_member")));
    expect_that!(opened_with_bad_bearer.status, eq(StatusCode::UNAUTHORIZED));
    let team = setup
        .app
        .send(Method::GET, &setup.team_url, Some(&setup.ana), None)
        .await?;
    expect_that!(role_of(&team.body, &sam_id), none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_expired_session_opens_the_link_as_signed_out() -> Result<()> {
    // Arrange
    let setup = setup().await?;
    let token = new_token(&setup).await?;
    let (sam, _) = newcomer(&setup, "sam@example.test", "Sam").await?;
    setup.app.clock.advance(Duration::from_days(31));

    // Act
    let opened = open(&setup, &token, Some(&sam)).await?;

    // Assert
    expect_that!(opened.status, eq(StatusCode::OK));
    expect_that!(opened.body["viewer"], eq(&Value::Null));
    Ok(())
}
