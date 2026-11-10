//! Heaps: creating, renaming, archiving and unarchiving them.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::json;

/// Registers Ada and returns her session and personal team ID.
async fn ada(app: &TestApp) -> Result<(String, String)> {
    let (session, _) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let team = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    Ok((session, team))
}

async fn create_heap(app: &TestApp, session: &str, team: &str, name: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/heaps", app.team_url(&team).await?),
        Some(session),
        Some(json!({ "name": name })),
    )
    .await
}

async fn post(app: &TestApp, session: &str, path: &str) -> Result<TestResponse> {
    app.send(Method::POST, path, Some(session), None).await
}

#[gtest]
#[tokio::test]
async fn a_new_heap_is_listed_for_every_member_of_its_team() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, team) = ada(&app).await?;

    // Act
    let created = create_heap(&app, &session, &team, "Ops").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let heap = created.body["id"].as_str().context("no id")?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(created.body["name"], eq(&json!("Ops")));
    expect_that!(created.body["archived_at"], eq(&json!(null)));
    expect_that!(
        created.body["allowed_actions"],
        eq(&json!([
            "rename",
            "archive",
            "edit_context",
            "edit_weights",
            "view_history"
        ]))
    );
    let names: Vec<&str> = me.body["teams"][0]["heaps"]
        .as_array()
        .map(|heaps| heaps.iter().filter_map(|h| h["name"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(names, elements_are![eq(&"Ops"), eq(&"Personal")]);
    expect_that!(list.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_is_read_only_until_unarchived() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, team) = ada(&app).await?;
    let created = create_heap(&app, &session, &team, "Ops").await?;
    let heap = created.body["id"].as_str().context("no id")?.to_owned();
    app.new_item(&session, &heap, json!({ "title": "Kept" })).await?;

    // Act
    let archived = post(&app, &session, &format!("{}/archive", app.heap_url(&heap).await?)).await?;
    let again = post(&app, &session, &format!("{}/archive", app.heap_url(&heap).await?)).await?;
    let write = app.create_item(&session, &heap, json!({ "title": "Refused" })).await?;
    let rename = app
        .patch(
            &session,
            &app.heap_url(&heap).await?,
            &archived.body["version"],
            json!({ "name": "X" }),
        )
        .await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let unarchived = post(&app, &session, &format!("{}/unarchive", app.heap_url(&heap).await?)).await?;
    let write_after = app.create_item(&session, &heap, json!({ "title": "Accepted" })).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(archived.body["archived_at"], eq(&json!(common::START)));
    expect_that!(
        archived.body["allowed_actions"],
        eq(&json!(["unarchive", "view_history"]))
    );
    expect_that!(again.body["version"], eq(&archived.body["version"]));
    expect_that!(write.body["code"], eq(&json!("heap_archived")));
    expect_that!(rename.body["code"], eq(&json!("heap_archived")));
    expect_that!(list.status, eq(StatusCode::OK));
    expect_that!(list.body["ranked"][0]["item"]["allowed_actions"], eq(&json!([])));
    expect_that!(unarchived.body["archived_at"], eq(&json!(null)));
    expect_that!(write_after.status, eq(StatusCode::CREATED));
    let entries = app.audit_entries(&heap).await?;
    let actions: Vec<&str> = entries.iter().map(|(a, _)| a.as_str()).collect();
    expect_that!(
        actions,
        elements_are![eq(&"heap_created"), eq(&"heap_archived"), eq(&"heap_unarchived")]
    );
    expect_that!(
        entries.get(1).map(|(_, diff)| diff),
        some(eq(
            &json!({ "archived_at": { "before": null, "after": common::START } })
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn renaming_a_heap_needs_its_current_version() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, team) = ada(&app).await?;
    let created = create_heap(&app, &session, &team, "Ops").await?;
    let heap = created.body["id"].as_str().context("no id")?.to_owned();
    let path = app.heap_url(&heap).await?;

    // Act
    let renamed = app
        .patch(&session, &path, &created.body["version"], json!({ "name": "Infra" }))
        .await?;
    let stale = app
        .patch(&session, &path, &created.body["version"], json!({ "name": "Ops 2" }))
        .await?;
    let fetched = app.send(Method::GET, &path, Some(&session), None).await?;

    // Assert
    expect_that!(renamed.body["name"], eq(&json!("Infra")));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(fetched.body["name"], eq(&json!("Infra")));
    expect_that!(fetched.body["version"], eq(&renamed.body["version"]));
    Ok(())
}

/// A shared team's admin and a plain member, the heap's URL and the team's ID.
struct Shared {
    admin: String,
    member: String,
    path: String,
    team: String,
}

async fn shared(app: &TestApp) -> Result<Shared> {
    let (admin, _) = app.register("admin@example.test").await?;
    let (member, _) = app.register("member@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let user = me.body["user"]["id"].as_str().context("no user")?;
    app.add_member(&team, user, domain::team::Role::Member).await?;
    let path = app.heap_url(&heap).await?;
    Ok(Shared {
        admin,
        member,
        path,
        team,
    })
}

async fn heap_of(app: &TestApp, session: &str, path: &str) -> Result<serde_json::Value> {
    Ok(app.send(Method::GET, path, Some(session), None).await?.body)
}

#[gtest]
#[tokio::test]
async fn a_member_sets_and_clears_the_heaps_context() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let world = shared(&app).await?;
    let before = heap_of(&app, &world.member, &world.path).await?;

    // Act
    let set = app
        .patch(
            &world.member,
            &world.path,
            &before["version"],
            json!({ "context": "  Docs are in the wiki.\n\n\tReview on GitHub.  " }),
        )
        .await?;
    let cleared = app
        .patch(
            &world.member,
            &world.path,
            &set.body["version"],
            json!({ "context": null }),
        )
        .await?;
    let set_again = app
        .patch(
            &world.member,
            &world.path,
            &cleared.body["version"],
            json!({ "context": "x" }),
        )
        .await?;
    let blanked = app
        .patch(
            &world.member,
            &world.path,
            &set_again.body["version"],
            json!({ "context": "  \n " }),
        )
        .await?;

    // Assert
    expect_that!(before["context"], eq(&json!(null)));
    expect_that!(set.status, eq(StatusCode::OK));
    expect_that!(
        set.body["context"],
        eq(&json!("Docs are in the wiki.\n\n\tReview on GitHub."))
    );
    expect_that!(
        set.body["version"],
        eq(&json!(before["version"].as_u64().context("version")? + 1))
    );
    expect_that!(cleared.body["context"], eq(&json!(null)));
    expect_that!(blanked.body["context"], eq(&json!(null)));
    expect_that!(set.body["allowed_actions"], eq(&json!(["edit_context"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_same_context_again_changes_nothing() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let world = shared(&app).await?;
    let before = heap_of(&app, &world.member, &world.path).await?;
    let set = app
        .patch(
            &world.member,
            &world.path,
            &before["version"],
            json!({ "context": "Same" }),
        )
        .await?;
    let heap_id = set.body["id"].as_str().context("no id")?.to_owned();
    let entries_before = app.audit_entries(&heap_id).await?.len();

    // Act
    let again = app
        .patch(
            &world.member,
            &world.path,
            &set.body["version"],
            json!({ "context": " Same " }),
        )
        .await?;
    let none = app
        .patch(&world.admin, &world.path, &set.body["version"], json!({}))
        .await?;
    let blank_when_none = {
        let cleared = app
            .patch(
                &world.member,
                &world.path,
                &set.body["version"],
                json!({ "context": null }),
            )
            .await?;
        app.patch(
            &world.member,
            &world.path,
            &cleared.body["version"],
            json!({ "context": "" }),
        )
        .await?
        .body["version"]
            .clone()
            == cleared.body["version"]
    };

    // Assert
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body["version"], eq(&set.body["version"]));
    expect_that!(none.body["version"], eq(&set.body["version"]));
    expect_that!(app.audit_entries(&heap_id).await?.len(), eq(entries_before + 1));
    expect_that!(blank_when_none, eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_cannot_send_the_name_and_nothing_is_saved() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let world = shared(&app).await?;
    let before = heap_of(&app, &world.member, &world.path).await?;

    // Act
    let name = app
        .patch(
            &world.member,
            &world.path,
            &before["version"],
            json!({ "name": "Mine" }),
        )
        .await?;
    let both = app
        .patch(
            &world.member,
            &world.path,
            &before["version"],
            json!({ "name": "Mine", "context": "Hello" }),
        )
        .await?;
    let after = heap_of(&app, &world.member, &world.path).await?;

    // Assert
    expect_that!(name.status, eq(StatusCode::FORBIDDEN));
    expect_that!(both.status, eq(StatusCode::FORBIDDEN));
    expect_that!(after, eq(&before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_admin_saves_the_name_and_context_in_one_version_with_two_entries() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let world = shared(&app).await?;
    let before = heap_of(&app, &world.admin, &world.path).await?;

    // Act
    let saved = app
        .patch(
            &world.admin,
            &world.path,
            &before["version"],
            json!({ "name": "Renamed", "context": "About it" }),
        )
        .await?;

    // Assert
    let heap_id = saved.body["id"].as_str().context("no id")?;
    let entries = app.audit_entries(heap_id).await?;
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(saved.body["name"], eq(&json!("Renamed")));
    expect_that!(saved.body["context"], eq(&json!("About it")));
    expect_that!(
        saved.body["version"],
        eq(&json!(before["version"].as_u64().context("version")? + 1))
    );
    expect_that!(
        actions,
        elements_are![eq(&"heap_created"), eq(&"heap_updated"), eq(&"heap_context_updated")]
    );
    expect_that!(
        entries.last().map(|(_, diff)| diff),
        some(eq(&json!({ "context": { "before": null, "after": "About it" } })))
    );
    expect_that!(
        saved.body["allowed_actions"],
        eq(&json!([
            "rename",
            "archive",
            "edit_context",
            "edit_weights",
            "view_history"
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_context_is_refused_on_an_archived_heap_too_long_or_stale() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let world = shared(&app).await?;
    let before = heap_of(&app, &world.member, &world.path).await?;
    let exact = "é".repeat(20_000);

    // Act
    let at_limit = app
        .patch(
            &world.member,
            &world.path,
            &before["version"],
            json!({ "context": exact }),
        )
        .await?;
    let too_long = app
        .patch(
            &world.member,
            &world.path,
            &at_limit.body["version"],
            json!({ "context": "x".repeat(20_001) }),
        )
        .await?;
    let stale = app
        .patch(
            &world.member,
            &world.path,
            &before["version"],
            json!({ "context": "Late" }),
        )
        .await?;
    let heap_id = before["id"].as_str().context("no id")?;
    app.set_heap_archived(heap_id, true).await?;
    let archived_current = heap_of(&app, &world.member, &world.path).await?;
    let on_archived_member = app
        .patch(
            &world.member,
            &world.path,
            &archived_current["version"],
            json!({ "context": "No" }),
        )
        .await?;
    let on_archived_admin = app
        .patch(
            &world.admin,
            &world.path,
            &archived_current["version"],
            json!({ "context": "No" }),
        )
        .await?;

    // Assert
    expect_that!(at_limit.status, eq(StatusCode::OK));
    expect_that!(too_long.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(too_long.body["field"], eq(&json!("context")));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(on_archived_member.body["code"], eq(&json!("heap_archived")));
    expect_that!(on_archived_admin.body["code"], eq(&json!("heap_archived")));
    expect_that!(archived_current["allowed_actions"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_context_is_on_the_heap_and_in_the_teams_heap_list_and_outsiders_get_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let world = shared(&app).await?;
    let (outsider, _) = app.register("outsider@example.test").await?;
    let before = heap_of(&app, &world.admin, &world.path).await?;
    app.patch(
        &world.member,
        &world.path,
        &before["version"],
        json!({ "context": "About it" }),
    )
    .await?;

    // Act
    let team = app
        .send(
            Method::GET,
            &app.team_url(&world.team).await?,
            Some(&world.member),
            None,
        )
        .await?;
    let fetched = heap_of(&app, &world.member, &world.path).await?;
    let hidden = app
        .patch(
            &outsider,
            &world.path,
            &fetched["version"],
            json!({ "context": "Mine" }),
        )
        .await?;

    // Assert
    expect_that!(fetched["context"], eq(&json!("About it")));
    expect_that!(team.body["heaps"][0]["context"], eq(&json!("About it")));
    expect_that!(team.body["heaps"][0]["allowed_actions"], eq(&json!(["edit_context"])));
    expect_that!(hidden.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}
