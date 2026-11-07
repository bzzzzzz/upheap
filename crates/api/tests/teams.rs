//! Teams: creating and renaming them, adding, promoting and removing members, leaving, looking users up, and the
//! display name.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

async fn user_id(app: &TestApp, session: &str) -> Result<String> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    Ok(me.body["user"]["id"].as_str().context("user has no id")?.to_owned())
}

async fn create_team(app: &TestApp, session: &str, name: &str) -> Result<Value> {
    let created = app
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(session),
            Some(json!({ "name": name })),
        )
        .await?;
    anyhow::ensure!(
        created.status == StatusCode::CREATED,
        "create team failed: {}",
        created.body
    );
    Ok(created.body)
}

async fn get_team(app: &TestApp, session: &str, team: &str) -> Result<TestResponse> {
    app.send(Method::GET, &app.team_url(&team).await?, Some(session), None)
        .await
}

async fn add_member(app: &TestApp, session: &str, team: &str, user: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/members", app.team_url(&team).await?),
        Some(session),
        Some(json!({ "user_id": user })),
    )
    .await
}

async fn set_role(app: &TestApp, session: &str, team: &str, user: &str, role: &str) -> Result<TestResponse> {
    app.send(
        Method::PATCH,
        &app.member_url(&team, &user).await?,
        Some(session),
        Some(json!({ "role": role })),
    )
    .await
}

async fn leave(app: &TestApp, session: &str, team: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/leave", app.team_url(&team).await?),
        Some(session),
        None,
    )
    .await
}

/// The member of `team` with `user`'s ID.
fn member<'a>(team: &'a Value, user: &str) -> &'a Value {
    team["members"]
        .as_array()
        .and_then(|members| members.iter().find(|m| m["user_id"] == user))
        .unwrap_or(&Value::Null)
}

#[gtest]
#[tokio::test]
async fn a_new_team_has_its_creator_as_admin_and_no_heaps() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register_as("ada@example.test", "Ada").await?;

    // Act
    let team = create_team(&app, &session, "  Platform  ").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;

    // Assert
    expect_that!(team["name"], eq(&json!("Platform")));
    expect_that!(team["role"], eq(&json!("admin")));
    expect_that!(team["heaps"], eq(&json!([])));
    expect_that!(team["members"][0]["display_name"], eq(&json!("Ada")));
    expect_that!(team["members"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(
        team["allowed_actions"],
        eq(&json!(["rename", "change_slug", "add_member", "create_heap"]))
    );
    let names: Vec<&str> = me.body["teams"]
        .as_array()
        .map(|teams| teams.iter().filter_map(|t| t["name"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(names, elements_are![eq(&"Ada"), eq(&"Platform")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_added_member_sees_the_team_and_its_heaps() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada").await?;
    let (grace, _) = app.register_as("grace@example.test", "Grace").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let grace_id = user_id(&app, &grace).await?;

    // Act
    let added = add_member(&app, &admin, &team, &grace_id).await?;
    let again = add_member(&app, &admin, &team, &grace_id).await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&grace),
            None,
        )
        .await?;
    let as_grace = get_team(&app, &grace, &team).await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::CREATED));
    expect_that!(member(&added.body, &grace_id)["role"], eq(&json!("member")));
    expect_that!(
        member(&added.body, &grace_id)["allowed_actions"],
        eq(&json!(["promote", "remove"]))
    );
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body["members"].as_array().map(Vec::len), some(eq(2)));
    expect_that!(list.status, eq(StatusCode::OK));
    expect_that!(as_grace.body["role"], eq(&json!("member")));
    expect_that!(as_grace.body["allowed_actions"], eq(&json!(["leave"])));
    expect_that!(member(&as_grace.body, &grace_id)["allowed_actions"], eq(&json!([])));
    expect_that!(
        app.audit_entries(&grace_id).await?.last(),
        some(eq(&("member_added".to_owned(), json!({ "role": "member" }))))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn adding_a_user_that_does_not_exist_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register("ada@example.test").await?;
    let team = create_team(&app, &admin, "Platform").await?;
    let team = team["id"].as_str().context("no id")?;

    // Act
    let added = add_member(&app, &admin, team, "0190f0f0-0000-7000-8000-000000000000").await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(added.body["code"], eq(&json!("user_not_found")));
    expect_that!(added.body["field"], eq(&json!("user_id")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_last_admin_cannot_leave_be_demoted_or_removed_until_another_is_promoted() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let (ada_id, grace_id) = (user_id(&app, &ada).await?, user_id(&app, &grace).await?);
    add_member(&app, &ada, &team, &grace_id).await?;

    // Act
    let left = leave(&app, &ada, &team).await?;
    let demoted = set_role(&app, &ada, &team, &ada_id, "member").await?;
    let removed = app
        .send(Method::DELETE, &app.member_url(&team, &ada_id).await?, Some(&ada), None)
        .await?;
    let promoted = set_role(&app, &ada, &team, &grace_id, "admin").await?;
    let left_after = leave(&app, &ada, &team).await?;
    let as_grace = get_team(&app, &grace, &team).await?;

    // Assert
    for refused in [&left, &demoted, &removed] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(refused.body["code"], eq(&json!("last_admin")));
    }
    expect_that!(promoted.status, eq(StatusCode::OK));
    expect_that!(member(&promoted.body, &grace_id)["role"], eq(&json!("admin")));
    expect_that!(
        promoted.body["allowed_actions"],
        eq(&json!(["rename", "change_slug", "add_member", "create_heap", "leave"]))
    );
    expect_that!(left_after.status, eq(StatusCode::NO_CONTENT));
    expect_that!(get_team(&app, &ada, &team).await?.status, eq(StatusCode::NOT_FOUND));
    expect_that!(as_grace.body["members"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(
        as_grace.body["allowed_actions"],
        eq(&json!(["rename", "change_slug", "add_member", "create_heap"]))
    );
    let actions: Vec<String> = app.audit_entries(&ada_id).await?.into_iter().map(|(a, _)| a).collect();
    expect_that!(actions.last(), some(eq("member_left")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_removed_member_loses_access_to_the_heaps() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let (team, heap) = app.shared_heap(&ada).await?;
    let grace_id = user_id(&app, &grace).await?;
    add_member(&app, &ada, &team, &grace_id).await?;
    let path = app.member_url(&team, &grace_id).await?;

    // Act
    let removed = app.send(Method::DELETE, &path, Some(&ada), None).await?;
    let again = app.send(Method::DELETE, &path, Some(&ada), None).await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&grace),
            None,
        )
        .await?;

    // Assert
    expect_that!(removed.status, eq(StatusCode::NO_CONTENT));
    expect_that!(again.status, eq(StatusCode::NO_CONTENT));
    expect_that!(list.status, eq(StatusCode::NOT_FOUND));
    let actions: Vec<String> = app
        .audit_entries(&grace_id)
        .await?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    expect_that!(actions.last(), some(eq("member_removed")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_can_leave() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let (team, heap) = app.shared_heap(&ada).await?;
    add_member(&app, &ada, &team, &user_id(&app, &grace).await?).await?;

    // Act
    let left = leave(&app, &grace, &team).await?;
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&grace),
            None,
        )
        .await?;
    let again = leave(&app, &grace, &team).await?;

    // Assert
    expect_that!(left.status, eq(StatusCode::NO_CONTENT));
    expect_that!(list.status, eq(StatusCode::NOT_FOUND));
    expect_that!(again.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_the_same_role_changes_nothing() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let grace_id = user_id(&app, &grace).await?;
    add_member(&app, &ada, &team, &grace_id).await?;
    let before = app.audit_entries(&grace_id).await?.len();

    // Act
    let same = set_role(&app, &ada, &team, &grace_id, "member").await?;

    // Assert
    expect_that!(same.status, eq(StatusCode::OK));
    expect_that!(app.audit_entries(&grace_id).await?.len(), eq(before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn renaming_a_team_needs_its_current_version() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let id = team["id"].as_str().context("no id")?.to_owned();
    let path = app.team_url(&id).await?;

    // Act
    let renamed = app
        .patch(&ada, &path, &team["version"], json!({ "name": "Infra" }))
        .await?;
    let stale = app
        .patch(&ada, &path, &team["version"], json!({ "name": "Ops" }))
        .await?;
    let empty = app
        .patch(&ada, &path, &renamed.body["version"], json!({ "name": "  " }))
        .await?;

    // Assert
    expect_that!(renamed.status, eq(StatusCode::OK));
    expect_that!(renamed.body["name"], eq(&json!("Infra")));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(empty.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(empty.body["field"], eq(&json!("name")));
    expect_that!(
        app.audit_entries(&id).await?.last(),
        some(eq(&(
            "team_updated".to_owned(),
            json!({ "name": { "before": "Platform", "after": "Infra" } })
        )))
    );
    Ok(())
}

/// The display names in a `GET /users` response, and the emails shown with them.
fn found(response: &TestResponse) -> Vec<(String, Value)> {
    response.body["users"]
        .as_array()
        .map(|users| {
            users
                .iter()
                .map(|u| {
                    (
                        u["display_name"].as_str().unwrap_or_default().to_owned(),
                        u["email"].clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn admins_find_users_by_the_start_of_a_name_or_of_an_email() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada Lovelace").await?;
    app.shared_heap(&ada).await?;
    app.register_as("grace@navy.test", "Grace Hopper").await?;
    app.register_as("percent@example.test", "100% Sure").await?;
    app.register_as("under@example.test", "A_bc Two").await?;
    app.register_as("plain@example.test", "Axbc One").await?;
    let find = |query: &str| {
        let path = format!("/api/v1/users?query={query}");
        let session = ada.clone();
        let app = &app;
        async move { app.send(Method::GET, &path, Some(&session), None).await }
    };

    // Act
    let by_last_name = find("HOPPER").await?;
    let inside_a_word = find("opper").await?;
    let by_email = find("grace@navy").await?;
    let email_without_local_part = find("%40navy").await?;
    let email_text_without_at = find("example.test").await?;
    let percent = find("100%25").await?;
    let only_percents = find("%25%25%25").await?;
    let underscore = find("a_b").await?;
    let short = find("a-").await?;

    // Assert
    let grace = || ("Grace Hopper".to_owned(), json!(null));
    expect_that!(found(&by_last_name), elements_are![eq(&grace())]);
    expect_that!(found(&inside_a_word), is_empty());
    expect_that!(
        found(&by_email),
        elements_are![eq(&("Grace Hopper".to_owned(), json!("grace@navy.test")))]
    );
    expect_that!(found(&email_without_local_part), is_empty());
    expect_that!(found(&email_text_without_at), is_empty());
    expect_that!(
        found(&percent),
        elements_are![eq(&("100% Sure".to_owned(), json!(null)))]
    );
    expect_that!(found(&only_percents), is_empty());
    expect_that!(
        found(&underscore),
        elements_are![eq(&("A_bc Two".to_owned(), json!(null)))]
    );
    expect_that!(short.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(short.body["code"], eq(&json!("field_too_short")));
    expect_that!(short.body["field"], eq(&json!("query")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_user_who_is_no_shared_teams_admin_cannot_look_users_up() -> Result<()> {
    // Arrange: Grace makes a team, hands it to Ada and steps down, so she administers only her personal team, which
    // takes no members.
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let team = create_team(&app, &grace, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let (ada_id, grace_id) = (user_id(&app, &ada).await?, user_id(&app, &grace).await?);
    add_member(&app, &grace, &team, &ada_id).await?;
    set_role(&app, &grace, &team, &ada_id, "admin").await?;
    let stepped_down = set_role(&app, &grace, &team, &grace_id, "member").await?;

    // Act
    let found = app
        .send(Method::GET, "/api/v1/users?query=tester", Some(&grace), None)
        .await?;

    // Assert
    expect_that!(stepped_down.body["role"], eq(&json!("member")));
    expect_that!(found.status, eq(StatusCode::FORBIDDEN));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn changing_the_display_name_shows_in_every_team() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let me = app.send(Method::GET, "/api/v1/me", Some(&ada), None).await?;
    let version = me.body["user"]["version"].clone();
    let ada_id = user_id(&app, &ada).await?;

    // Act
    let changed = app
        .patch(&ada, "/api/v1/me", &version, json!({ "display_name": "Countess" }))
        .await?;
    let stale = app
        .patch(&ada, "/api/v1/me", &version, json!({ "display_name": "Ada L." }))
        .await?;
    let without_version = app
        .send_with(
            Method::PATCH,
            "/api/v1/me",
            Some(&ada),
            Some(json!({ "display_name": "Ada L." })),
            &[CSRF],
        )
        .await?;
    let as_team = get_team(&app, &ada, &team).await?;

    // Assert
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(changed.body["display_name"], eq(&json!("Countess")));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(without_version.status, eq(StatusCode::PRECONDITION_REQUIRED));
    expect_that!(member(&as_team.body, &ada_id)["display_name"], eq(&json!("Countess")));
    let renames = app
        .audit_entries(&ada_id)
        .await?
        .into_iter()
        .filter(|(action, _)| action == "user_updated")
        .count();
    // Once in her personal team and once in Platform.
    expect_that!(renames, eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_admin_removing_themselves_is_recorded_as_leaving() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let (ada_id, grace_id) = (user_id(&app, &ada).await?, user_id(&app, &grace).await?);
    add_member(&app, &ada, &team, &grace_id).await?;
    set_role(&app, &ada, &team, &grace_id, "admin").await?;

    // Act
    let removed = app
        .send(Method::DELETE, &app.member_url(&team, &ada_id).await?, Some(&ada), None)
        .await?;

    // Assert
    expect_that!(removed.status, eq(StatusCode::NO_CONTENT));
    let actions: Vec<String> = app.audit_entries(&ada_id).await?.into_iter().map(|(a, _)| a).collect();
    expect_that!(actions.last(), some(eq("member_left")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_and_demoted_admins_cannot_manage_the_team() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let (alan, _) = app.register("alan@example.test").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let grace_id = user_id(&app, &grace).await?;
    let alan_id = user_id(&app, &alan).await?;
    add_member(&app, &ada, &team, &grace_id).await?;
    set_role(&app, &ada, &team, &grace_id, "admin").await?;
    set_role(&app, &ada, &team, &grace_id, "member").await?;

    // Act
    let added = add_member(&app, &grace, &team, &alan_id).await?;
    let removed_self = app
        .send(
            Method::DELETE,
            &app.member_url(&team, &grace_id).await?,
            Some(&grace),
            None,
        )
        .await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::FORBIDDEN));
    expect_that!(removed_self.status, eq(StatusCode::FORBIDDEN));
    Ok(())
}

/// Two admins leaving at the same moment: one goes, the other is refused, and the team keeps an admin. On MySQL this
/// needs the membership read after the team lock (a read before it would show the other admin still there).
#[gtest]
#[tokio::test]
async fn two_admins_leaving_at_once_leave_one_behind() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let team = create_team(&app, &ada, "Platform").await?;
    let team = team["id"].as_str().context("no id")?.to_owned();
    let grace_id = user_id(&app, &grace).await?;
    add_member(&app, &ada, &team, &grace_id).await?;
    set_role(&app, &ada, &team, &grace_id, "admin").await?;

    // Act
    let (ada_left, grace_left) = tokio::join!(leave(&app, &ada, &team), leave(&app, &grace, &team));
    let (ada_left, grace_left) = (ada_left?, grace_left?);

    // Assert
    let mut statuses = [ada_left.status, grace_left.status];
    statuses.sort();
    expect_that!(statuses, eq([StatusCode::NO_CONTENT, StatusCode::CONFLICT]));
    let stayed = if ada_left.status == StatusCode::CONFLICT {
        &ada
    } else {
        &grace
    };
    let remaining = get_team(&app, stayed, &team).await?;
    expect_that!(remaining.body["members"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(remaining.body["role"], eq(&json!("admin")));
    Ok(())
}
