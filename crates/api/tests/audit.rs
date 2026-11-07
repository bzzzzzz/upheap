//! Reading the audit log: a heap's and a team's history, one entity's history, who sees what, and paging.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

fn actions(log: &Value) -> Vec<&str> {
    log["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["action"].as_str()).collect())
        .unwrap_or_default()
}

/// The caller's first team and their user ID, from `/me`.
async fn team_and_user(app: &TestApp, session: &str) -> Result<(String, String)> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    let team = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    let user = me.body["user"]["id"].as_str().context("no user")?.to_owned();
    Ok((team, user))
}

#[gtest]
#[tokio::test]
async fn an_items_history_reads_newest_first_with_names_for_ids() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register_as("ada@example.test", "Ada").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let item = app
        .new_item(&session, &heap, json!({ "title": "Rotate keys", "effort": "m" }))
        .await?;
    let id = item["id"].as_str().context("no id")?;
    app.patch(
        &session,
        &format!("/api/v1/items/{id}"),
        &item["version"],
        json!({ "title": "Rotate all keys", "state": "in_progress", "tag_ids": [ops] }),
    )
    .await?;
    app.new_item(&session, &heap, json!({ "title": "Something else" }))
        .await?;

    // Act
    let history = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/audit?entity_id={id}"),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(history.status, eq(StatusCode::OK));
    expect_that!(
        actions(&history.body),
        elements_are![eq(&"item_updated"), eq(&"item_created")]
    );
    let edit = &history.body["entries"][0];
    expect_that!(edit["actor"]["display_name"], eq(&json!("Ada")));
    expect_that!(
        edit["entity"],
        eq(&json!({ "type": "item", "id": id, "label": "Rotate all keys" }))
    );
    expect_that!(
        edit["changes"],
        eq(&json!([
            { "field": "title", "label": "Title", "before": "Rotate keys", "after": "Rotate all keys" },
            { "field": "state", "label": "State", "before": "Open", "after": "In progress" },
            { "field": "tags", "label": "Tags", "before": null, "after": "ops" },
        ]))
    );
    expect_that!(edit["diff"]["tags"]["after"], eq(&json!([ops])));
    expect_that!(
        history.body["entries"][1]["changes"],
        eq(&json!([
            { "field": "title", "label": "Title", "before": null, "after": "Rotate keys" },
            { "field": "state", "label": "State", "before": null, "after": "Open" },
            { "field": "effort", "label": "Effort", "before": null, "after": "M" },
        ]))
    );
    expect_that!(history.body["next_cursor"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_see_content_changes_and_admins_see_configuration_and_membership() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, heap) = app.register("admin@example.test").await?;
    let (member, _) = app.register("member@example.test").await?;
    let (team, _) = team_and_user(&app, &admin).await?;
    let (_, member_id) = team_and_user(&app, &member).await?;
    app.add_member(&team, &member_id, Role::Member).await?;
    app.new_item(&admin, &heap, json!({ "title": "Item" })).await?;
    let created = app
        .send(
            Method::POST,
            &format!("/api/v1/heaps/{heap}/parameters"),
            Some(&admin),
            Some(json!({ "name": "Customer" })),
        )
        .await?;
    anyhow::ensure!(created.status == StatusCode::CREATED, "parameter: {}", created.body);
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    app.patch(
        &member,
        "/api/v1/me",
        &me.body["user"]["version"],
        json!({ "display_name": "Mem" }),
    )
    .await?;

    // Act
    let heap_log = |session: String| {
        let app = &app;
        let path = format!("/api/v1/heaps/{heap}/audit");
        async move { app.send(Method::GET, &path, Some(&session), None).await }
    };
    let team_log = |session: String| {
        let app = &app;
        let path = format!("/api/v1/teams/{team}/audit");
        async move { app.send(Method::GET, &path, Some(&session), None).await }
    };
    let admin_heap = heap_log(admin.clone()).await?;
    let member_heap = heap_log(member.clone()).await?;
    let admin_team = team_log(admin.clone()).await?;
    let member_team = team_log(member.clone()).await?;

    // Assert
    expect_that!(
        actions(&admin_heap.body),
        elements_are![eq(&"parameter_created"), eq(&"item_created"), eq(&"heap_created")]
    );
    expect_that!(actions(&member_heap.body), elements_are![eq(&"item_created")]);
    expect_that!(
        actions(&admin_team.body),
        elements_are![
            eq(&"user_updated"),
            eq(&"heap_created"),
            eq(&"member_added"),
            eq(&"team_created"),
            eq(&"user_registered")
        ]
    );
    expect_that!(member_team.status, eq(StatusCode::OK));
    expect_that!(
        actions(&member_team.body),
        elements_are![eq(&"user_updated"), eq(&"user_registered")]
    );
    expect_that!(
        member_team.body["entries"][0]["changes"],
        eq(&json!([{ "field": "display_name", "label": "Display name", "before": "Tester", "after": "Mem" }]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn pages_follow_on_without_gaps_or_repeats() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    for n in 0..4 {
        app.new_item(&session, &heap, json!({ "title": format!("Item {n}") }))
            .await?;
    }

    // Act
    let mut labels = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let after = cursor.as_ref().map(|c| format!("&cursor={c}")).unwrap_or_default();
        let page = app
            .send(
                Method::GET,
                &format!("/api/v1/heaps/{heap}/audit?limit=2{after}"),
                Some(&session),
                None,
            )
            .await?;
        anyhow::ensure!(page.status == StatusCode::OK, "page: {}", page.body);
        pages += 1;
        for entry in page.body["entries"].as_array().context("no entries")? {
            labels.push(entry["entity"]["label"].as_str().unwrap_or_default().to_owned());
        }
        match page.body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
    }
    let invalid = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/audit?cursor=nonsense"),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(pages, eq(3));
    expect_that!(
        labels,
        elements_are![eq("Item 3"), eq("Item 2"), eq("Item 1"), eq("Item 0"), eq("Tester")]
    );
    expect_that!(invalid.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(invalid.body["code"], eq(&json!("invalid_cursor")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_teams_log_has_its_heaps_as_wholes_but_not_what_is_in_them() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (team, _) = team_and_user(&app, &session).await?;
    app.new_item(&session, &heap, json!({ "title": "Item" })).await?;
    let current = app
        .send(Method::GET, &format!("/api/v1/heaps/{heap}"), Some(&session), None)
        .await?;
    app.patch(
        &session,
        &format!("/api/v1/heaps/{heap}"),
        &current.body["version"],
        json!({ "name": "Work" }),
    )
    .await?;

    // Act
    let heap_history = app
        .send(
            Method::GET,
            &format!("/api/v1/teams/{team}/audit?entity_id={heap}"),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(
        actions(&heap_history.body),
        elements_are![eq(&"heap_updated"), eq(&"heap_created")]
    );
    expect_that!(heap_history.body["entries"][0]["entity"]["label"], eq(&json!("Work")));
    expect_that!(
        heap_history.body["entries"][0]["changes"],
        eq(&json!([{ "field": "name", "label": "Name", "before": "Tester", "after": "Work" }]))
    );
    Ok(())
}
