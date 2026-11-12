//! Saved views: saving a view of the list, sharing it with the team, changing it and archiving it.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// The caller's user ID and the ID of their personal team.
async fn me(app: &TestApp, session: &str) -> Result<(String, String)> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    let user = me.body["user"]["id"].as_str().context("no user id")?.to_owned();
    let team = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    Ok((user, team))
}

async fn save_view(app: &TestApp, session: &str, heap: &str, body: Value) -> Result<common::TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/views", app.heap_url(&heap).await?),
        Some(session),
        Some(body),
    )
    .await
}

async fn list_views(app: &TestApp, session: &str, heap: &str) -> Result<Value> {
    let list = app
        .send(
            Method::GET,
            &format!("{}/views", app.heap_url(&heap).await?),
            Some(session),
            None,
        )
        .await?;
    anyhow::ensure!(list.status == StatusCode::OK, "list views failed: {}", list.body);
    Ok(list.body)
}

fn names(list: &Value) -> Vec<&str> {
    list["views"]
        .as_array()
        .map(|views| views.iter().filter_map(|v| v["name"].as_str()).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn a_saved_view_spells_out_every_setting_and_the_team_sees_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (member_session, _) = app.register("bob@example.test").await?;
    let (ada, _) = me(&app, &session).await?;
    let (team, heap) = app.shared_heap(&session).await?;
    let (bob, _) = me(&app, &member_session).await?;
    app.add_member(&team, &bob, Role::Member).await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;

    // Act
    let saved = save_view(
        &app,
        &session,
        &heap,
        json!({ "name": " Ops work ", "view": { "blocked": "shown", "tag": [ops, ops], "effort": ["s"], "need": ["critical", "idea"] } }),
    )
    .await?;
    let listed = list_views(&app, &member_session, &heap).await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::CREATED));
    expect_that!(saved.body["name"], eq(&json!("Ops work")));
    expect_that!(
        saved.body["view"],
        eq(&json!({
            "open": "shown",
            "in_progress": "pinned",
            "blocked": "shown",
            "blocked_layout": "grouped",
            "reminders": "hidden",
            "sort": "priority",
            "sort_parameter": null,
            "tag": [ops],
            "effort": ["s"],
            "impact": [],
            "need": ["idea", "critical"],
            "param": [],
        }))
    );
    expect_that!(saved.body["created_by"], eq(&json!(ada)));
    expect_that!(saved.body["archived_at"], eq(&Value::Null));
    expect_that!(saved.body["allowed_actions"], eq(&json!(["edit", "archive"])));
    expect_that!(names(&listed), elements_are![eq(&"Ops work")]);
    expect_that!(listed["allowed_actions"], eq(&json!(["create"])));
    expect_that!(listed["views"][0]["allowed_actions"], eq(&json!(["edit", "archive"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn views_are_listed_by_name_ignoring_case() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    for name in ["beta", "Alpha", "Gamma"] {
        save_view(&app, &session, &heap, json!({ "name": name })).await?;
    }

    // Act
    let listed = list_views(&app, &session, &heap).await?;

    // Assert
    expect_that!(names(&listed), elements_are![eq(&"Alpha"), eq(&"beta"), eq(&"Gamma")]);
    expect_that!(listed["views"][0]["view"]["blocked"], eq(&json!("hidden")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn editing_a_view_needs_its_current_version_and_is_audited() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let saved = save_view(&app, &session, &heap, json!({ "name": "Ready" })).await?;
    let id = saved.body["id"].as_str().context("view has no id")?.to_owned();
    let path = format!("{}/views/{id}", app.heap_url(&heap).await?);

    // Act
    let edited = app
        .patch(
            &session,
            &path,
            &saved.body["version"],
            json!({ "name": "Most needed", "view": { "sort": "need" } }),
        )
        .await?;
    let stale = app
        .patch(&session, &path, &saved.body["version"], json!({ "name": "Late" }))
        .await?;
    let unchanged = app
        .patch(
            &session,
            &path,
            &edited.body["version"],
            json!({ "name": "Most needed" }),
        )
        .await?;
    let audit = app.audit_entries(&id).await?;

    // Assert
    expect_that!(edited.status, eq(StatusCode::OK));
    expect_that!(edited.body["name"], eq(&json!("Most needed")));
    expect_that!(edited.body["view"]["sort"], eq(&json!("need")));
    expect_that!(edited.body["version"], eq(&json!(2)));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(unchanged.status, eq(StatusCode::OK));
    expect_that!(unchanged.body["version"], eq(&json!(2)));
    let actions: Vec<&str> = audit.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![eq(&"saved_view_created"), eq(&"saved_view_updated")]
    );
    expect_that!(
        audit[1].1["name"],
        eq(&json!({ "before": "Ready", "after": "Most needed" }))
    );
    expect_that!(audit[1].1["list_view"]["before"]["sort"], eq(&json!("priority")));
    expect_that!(audit[1].1["list_view"]["after"]["sort"], eq(&json!("need")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_view_leaves_the_list_and_cant_be_changed() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let saved = save_view(&app, &session, &heap, json!({ "name": "Old" })).await?;
    save_view(&app, &session, &heap, json!({ "name": "Kept" })).await?;
    let id = saved.body["id"].as_str().context("view has no id")?.to_owned();
    let archive = format!("{}/views/{id}/archive", app.heap_url(&heap).await?);

    // Act
    let archived = app.send(Method::POST, &archive, Some(&session), None).await?;
    let again = app.send(Method::POST, &archive, Some(&session), None).await?;
    let listed = list_views(&app, &session, &heap).await?;
    let edit = app
        .patch(
            &session,
            &format!("{}/views/{id}", app.heap_url(&heap).await?),
            &archived.body["version"],
            json!({ "name": "Back" }),
        )
        .await?;
    let audit = app.audit_entries(&id).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(archived.body["archived_at"], eq(&json!(common::START)));
    expect_that!(archived.body["allowed_actions"], eq(&json!([])));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body["version"], eq(&archived.body["version"]));
    expect_that!(names(&listed), elements_are![eq(&"Kept")]);
    expect_that!(edit.status, eq(StatusCode::CONFLICT));
    expect_that!(edit.body["code"], eq(&json!("saved_view_archived")));
    let actions: Vec<&str> = audit.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![eq(&"saved_view_created"), eq(&"saved_view_archived")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_view_can_only_filter_by_tags_of_its_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    let foreign = app.new_tag_id(&other_session, &other_heap, "theirs").await?;
    let saved = save_view(&app, &session, &heap, json!({ "name": "Mine" })).await?;
    let path = format!(
        "{}/views/{}",
        app.heap_url(&heap).await?,
        saved.body["id"].as_str().unwrap_or_default()
    );

    // Act
    let created = save_view(
        &app,
        &session,
        &heap,
        json!({ "name": "Theirs", "view": { "tag": [foreign] } }),
    )
    .await?;
    let edited = app
        .patch(
            &session,
            &path,
            &saved.body["version"],
            json!({ "view": { "tag": [foreign] } }),
        )
        .await?;

    // Assert
    for response in [&created, &edited] {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(response.body["code"], eq(&json!("tag_not_in_heap")));
        expect_that!(response.body["field"], eq(&json!("view.tag")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn malformed_views_are_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;

    // Act
    let blank = save_view(&app, &session, &heap, json!({ "name": "  " })).await?;
    let unknown_setting = save_view(
        &app,
        &session,
        &heap,
        json!({ "name": "Typo", "view": { "blokced": "shown" } }),
    )
    .await?;
    let unknown_mode = save_view(
        &app,
        &session,
        &heap,
        json!({ "name": "Typo", "view": { "blocked": "floating" } }),
    )
    .await?;
    let unknown_field = save_view(&app, &session, &heap, json!({ "name": "Typo", "veiw": {} })).await?;
    let many: Vec<String> = (0..=domain::saved_view::MAX_VIEW_TAGS)
        .map(|n| uuid::Uuid::from_u128(n as u128).to_string())
        .collect();
    let too_many_tags = save_view(&app, &session, &heap, json!({ "name": "All", "view": { "tag": many } })).await?;

    // Assert
    expect_that!(blank.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(blank.body["code"], eq(&json!("field_empty")));
    expect_that!(unknown_setting.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(unknown_setting.body["code"], eq(&json!("invalid_body")));
    expect_that!(unknown_mode.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(unknown_field.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(unknown_field.body["code"], eq(&json!("invalid_body")));
    expect_that!(too_many_tags.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(too_many_tags.body["code"], eq(&json!("too_many_tags")));
    expect_that!(too_many_tags.body["field"], eq(&json!("view.tag")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_view_equal_to_the_saved_one_changes_nothing() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let web = app.new_tag_id(&session, &heap, "web").await?;
    let saved = save_view(
        &app,
        &session,
        &heap,
        json!({ "name": "Tagged", "view": { "sort": "due" } }),
    )
    .await?;
    let id = saved.body["id"].as_str().context("view has no id")?.to_owned();
    let path = format!("{}/views/{id}", app.heap_url(&heap).await?);

    // Act
    let tagged = app
        .patch(
            &session,
            &path,
            &saved.body["version"],
            json!({ "view": { "sort": "due", "tag": [web, ops, web] } }),
        )
        .await?;
    let same = app
        .patch(
            &session,
            &path,
            &tagged.body["version"],
            json!({ "view": { "tag": [ops, web], "sort": "due", "blocked": "hidden" } }),
        )
        .await?;
    let audit = app.audit_entries(&id).await?;

    // Assert
    let mut sorted = [ops.clone(), web.clone()];
    sorted.sort();
    expect_that!(tagged.status, eq(StatusCode::OK));
    expect_that!(tagged.body["view"]["tag"], eq(&json!(sorted)));
    expect_that!(same.status, eq(StatusCode::OK));
    expect_that!(same.body["version"], eq(&tagged.body["version"]));
    expect_that!(audit.len(), eq(2));
    Ok(())
}
