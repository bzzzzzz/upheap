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
        &app.item_url(&id).await?,
        &item["version"],
        json!({ "title": "Rotate all keys", "state": "in_progress", "need": "critical", "tag_ids": [ops] }),
    )
    .await?;
    app.new_item(&session, &heap, json!({ "title": "Something else" }))
        .await?;

    // Act
    let history = app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={id}", app.heap_url(&heap).await?),
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
            { "field": "need", "label": "Need", "before": "Expected", "after": "Critical" },
            { "field": "tags", "label": "Tags", "before": null, "after": "ops" },
        ]))
    );
    expect_that!(edit["diff"]["tags"]["after"], eq(&json!([ops])));
    expect_that!(
        history.body["entries"][1]["changes"],
        eq(&json!([
            { "field": "title", "label": "Title", "before": null, "after": "Rotate keys" },
            { "field": "state", "label": "State", "before": null, "after": "Open" },
            { "field": "need", "label": "Need", "before": null, "after": "Expected" },
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
    let (admin, _) = app.register("admin@example.test").await?;
    let (member, _) = app.register("member@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let (_, member_id) = team_and_user(&app, &member).await?;
    app.add_member(&team, &member_id, Role::Member).await?;
    app.new_item(&admin, &heap, json!({ "title": "Item" })).await?;
    let created = app
        .send(
            Method::POST,
            &format!("{}/parameters", app.heap_url(&heap).await?),
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
    let heap_path = format!("{}/audit", app.heap_url(&heap).await?);
    let team_path = format!("{}/audit", app.team_url(&team).await?);
    let heap_log = |session: String| {
        let (app, path) = (&app, &heap_path);
        async move { app.send(Method::GET, path, Some(&session), None).await }
    };
    let team_log = |session: String| {
        let (app, path) = (&app, &team_path);
        async move { app.send(Method::GET, path, Some(&session), None).await }
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
            eq(&"team_created")
        ]
    );
    expect_that!(member_team.status, eq(StatusCode::OK));
    expect_that!(actions(&member_team.body), elements_are![eq(&"user_updated")]);
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
                &format!("{}/audit?limit=2{after}", app.heap_url(&heap).await?),
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
            &format!("{}/audit?cursor=nonsense", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(pages, eq(3));
    expect_that!(
        labels,
        elements_are![eq("Item 3"), eq("Item 2"), eq("Item 1"), eq("Item 0"), eq("Personal")]
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
        .send(Method::GET, &app.heap_url(&heap).await?, Some(&session), None)
        .await?;
    app.patch(
        &session,
        &app.heap_url(&heap).await?,
        &current.body["version"],
        json!({ "name": "Work" }),
    )
    .await?;

    // Act
    let heap_history = app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={heap}", app.team_url(&team).await?),
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
        eq(&json!([{ "field": "name", "label": "Name", "before": "Personal", "after": "Work" }]))
    );
    Ok(())
}

async fn get(app: &TestApp, session: &str, path: &str) -> Result<Value> {
    let response = app.send(Method::GET, path, Some(session), None).await?;
    anyhow::ensure!(response.status == StatusCode::OK, "GET {path}: {}", response.body);
    Ok(response.body)
}

async fn post(app: &TestApp, session: &str, path: &str, body: Value) -> Result<Value> {
    let response = app.send(Method::POST, path, Some(session), Some(body)).await?;
    anyhow::ensure!(response.status.is_success(), "POST {path}: {}", response.body);
    Ok(response.body)
}

/// Each entry as "<actor> <phrase>[ <entity>]", the way the web shows it in a log of many entities.
fn sentences(log: &Value) -> Vec<String> {
    log["entries"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|e| {
                    let mut text = format!(
                        "{} {}",
                        e["actor"]["display_name"].as_str().unwrap_or_default(),
                        e["phrase"].as_str().unwrap_or_default()
                    );
                    if e["names_entity"] == json!(true) {
                        text = format!("{text} {}", e["entity"]["label"].as_str().unwrap_or_default());
                    }
                    text
                })
                .collect()
        })
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn an_entity_of_another_heap_or_team_has_no_history_here_and_members_get_none_for_parameters() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register("admin@example.test").await?;
    let (member, other_heap) = app.register("member@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let (_, member_id) = team_and_user(&app, &member).await?;
    app.add_member(&team, &member_id, Role::Member).await?;
    let elsewhere = app
        .new_item(&member, &other_heap, json!({ "title": "Private" }))
        .await?;
    let parameter = post(
        &app,
        &admin,
        &format!("{}/parameters", app.heap_url(&heap).await?),
        json!({ "name": "Customer" }),
    )
    .await?;

    // Act
    let other_item = get(
        &app,
        &member,
        &format!(
            "{}/audit?entity_id={}",
            app.heap_url(&heap).await?,
            elsewhere["id"].as_str().unwrap_or_default()
        ),
    )
    .await?;
    let other_heap_in_team = get(
        &app,
        &admin,
        &format!("{}/audit?entity_id={other_heap}", app.team_url(&team).await?),
    )
    .await?;
    let member_parameter = get(
        &app,
        &member,
        &format!(
            "{}/audit?entity_id={}",
            app.heap_url(&heap).await?,
            parameter["id"].as_str().unwrap_or_default()
        ),
    )
    .await?;
    let admin_parameter = get(
        &app,
        &admin,
        &format!(
            "{}/audit?entity_id={}",
            app.heap_url(&heap).await?,
            parameter["id"].as_str().unwrap_or_default()
        ),
    )
    .await?;

    // Assert
    expect_that!(actions(&other_item), is_empty());
    expect_that!(actions(&other_heap_in_team), is_empty());
    expect_that!(actions(&member_parameter), is_empty());
    expect_that!(actions(&admin_parameter), elements_are![eq(&"parameter_created")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archiving_an_option_reads_on_the_parameter_and_on_each_switched_item() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register_as("ada@example.test", "Ada").await?;
    let parameter = post(
        &app,
        &session,
        &format!("{}/parameters", app.heap_url(&heap).await?),
        json!({ "name": "Customer", "options": ["Small", "Big"] }),
    )
    .await?;
    let (pid, small, big) = (
        parameter["id"].as_str().unwrap_or_default().to_owned(),
        parameter["options"][0]["id"].as_str().unwrap_or_default().to_owned(),
        parameter["options"][1]["id"].as_str().unwrap_or_default().to_owned(),
    );
    let blocker = app.new_item(&session, &heap, json!({ "title": "Blocker" })).await?;
    let item = app
        .new_item(
            &session,
            &heap,
            json!({ "title": "Valued", "parameter_values": { pid.clone(): small } }),
        )
        .await?;
    let item_id = item["id"].as_str().unwrap_or_default().to_owned();
    app.patch(
        &session,
        &app.item_url(&item_id).await?,
        &item["version"],
        json!({ "state": "blocked", "blocked_by_item_id": blocker["id"] }),
    )
    .await?;
    post(
        &app,
        &session,
        &format!("{}/options/{small}/archive", app.heap_url(&heap).await?),
        json!({ "replacement_option_id": big }),
    )
    .await?;

    // Act
    let parameter_log = get(
        &app,
        &session,
        &format!("{}/audit?entity_id={pid}", app.heap_url(&heap).await?),
    )
    .await?;
    let item_log = get(
        &app,
        &session,
        &format!("{}/audit?entity_id={item_id}", app.heap_url(&heap).await?),
    )
    .await?;

    // Assert
    expect_that!(
        sentences(&parameter_log),
        elements_are![
            eq("Ada archived an option of Customer"),
            eq("Ada added parameter Customer")
        ]
    );
    expect_that!(
        parameter_log["entries"][0]["changes"],
        eq(&json!([
            { "field": "options", "label": "Options", "before": "Small, Big", "after": "Big" },
            { "field": "archived_options", "label": "Archived options", "before": null, "after": "Small" },
            { "field": "items_switched", "label": "Items switched", "before": null, "after": "1 item" },
        ]))
    );
    expect_that!(
        item_log["entries"][0]["changes"],
        eq(&json!([{ "field": format!("parameters.{pid}"), "label": "Customer", "before": "Small", "after": "Big" }]))
    );
    expect_that!(
        item_log["entries"][1]["changes"],
        eq(&json!([
            { "field": "state", "label": "State", "before": "Open", "after": "Blocked" },
            { "field": "blocked_by", "label": "Blocked by", "before": null, "after": "Blocker" },
        ]))
    );
    expect_that!(
        item_log["entries"][2]["changes"],
        eq(&json!([
            { "field": "title", "label": "Title", "before": null, "after": "Valued" },
            { "field": "state", "label": "State", "before": null, "after": "Open" },
            { "field": "need", "label": "Need", "before": null, "after": "Expected" },
            { "field": format!("parameters.{pid}"), "label": "Customer", "before": null, "after": "Small" },
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn membership_heap_and_view_entries_read_as_sentences() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register_as("ada@example.test", "Ada").await?;
    let (bob, _) = app.register_as("bob@example.test", "Bob").await?;
    let (cy, _) = app.register_as("cy@example.test", "Cy").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let (_, bob_id) = team_and_user(&app, &bob).await?;
    let (_, cy_id) = team_and_user(&app, &cy).await?;
    post(
        &app,
        &admin,
        &format!("{}/members", app.team_url(&team).await?),
        json!({ "user_id": bob_id }),
    )
    .await?;
    post(
        &app,
        &admin,
        &format!("{}/members", app.team_url(&team).await?),
        json!({ "user_id": cy_id }),
    )
    .await?;
    let removed = app
        .send(
            Method::DELETE,
            &app.member_url(&team, &bob_id).await?,
            Some(&admin),
            None,
        )
        .await?;
    anyhow::ensure!(removed.status.is_success(), "remove: {}", removed.body);
    post(&app, &cy, &format!("{}/leave", app.team_url(&team).await?), json!({})).await?;
    let view = post(
        &app,
        &admin,
        &format!("{}/views", app.heap_url(&heap).await?),
        json!({ "name": "Blocked", "view": { "blocked": "shown" } }),
    )
    .await?;
    post(
        &app,
        &admin,
        &format!(
            "{}/views/{}/archive",
            app.heap_url(&heap).await?,
            view["id"].as_str().unwrap_or_default()
        ),
        json!({}),
    )
    .await?;
    post(
        &app,
        &admin,
        &format!("{}/archive", app.heap_url(&heap).await?),
        json!({}),
    )
    .await?;

    // Act
    let team_log = get(&app, &admin, &format!("{}/audit", app.team_url(&team).await?)).await?;
    let (personal, _) = team_and_user(&app, &admin).await?;
    let personal_log = get(&app, &admin, &format!("{}/audit", app.team_url(&personal).await?)).await?;
    let view_log = get(
        &app,
        &admin,
        &format!(
            "{}/audit?entity_id={}",
            app.heap_url(&heap).await?,
            view["id"].as_str().unwrap_or_default()
        ),
    )
    .await?;

    // Assert
    expect_that!(
        sentences(&team_log),
        elements_are![
            eq("Ada archived heap Shared"),
            eq("Cy left the team"),
            eq("Ada removed member Bob"),
            eq("Ada added member Cy"),
            eq("Ada added member Bob"),
            eq("Ada created heap Shared"),
            eq("Ada joined the team"),
            eq("Ada created team Shared"),
        ]
    );
    expect_that!(
        sentences(&personal_log),
        elements_are![
            eq("Ada created heap Personal"),
            eq("Ada joined the team"),
            eq("Ada created team Ada"),
            eq("Ada signed up"),
        ]
    );
    expect_that!(team_log["entries"][0]["changes"], eq(&json!([])));
    expect_that!(
        team_log["entries"][1]["changes"],
        eq(&json!([{ "field": "role", "label": "Role", "before": "Member", "after": null }]))
    );
    expect_that!(
        sentences(&view_log),
        elements_are![eq("Ada archived view Blocked"), eq("Ada saved view Blocked")]
    );
    expect_that!(
        view_log["entries"][1]["changes"],
        eq(&json!([{ "field": "name", "label": "Name", "before": null, "after": "Blocked" }]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn pages_follow_on_across_different_times() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    for n in 0..3 {
        app.clock.advance(domain::Duration::from_hours(1));
        app.new_item(&session, &heap, json!({ "title": format!("Item {n}") }))
            .await?;
    }

    // Act
    let first = get(&app, &session, &format!("{}/audit?limit=2", app.heap_url(&heap).await?)).await?;
    let cursor = first["next_cursor"].as_str().context("no cursor")?;
    let second = get(
        &app,
        &session,
        &format!("{}/audit?limit=2&cursor={cursor}", app.heap_url(&heap).await?),
    )
    .await?;

    // Assert
    let labels = |log: &Value| -> Vec<String> {
        log["entries"]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| e["entity"]["label"].as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    expect_that!(labels(&first), elements_are![eq("Item 2"), eq("Item 1")]);
    expect_that!(labels(&second), elements_are![eq("Item 0"), eq("Personal")]);
    expect_that!(second["next_cursor"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_items_history_names_kind_and_stage_changes_and_leaves_the_defaults_out_of_its_creation() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = app.new_item(&session, &heap, json!({ "title": "Rotate keys" })).await?;
    let id = item["id"].as_str().context("no id")?;
    app.clock.advance(domain::Duration::from_millis(1));
    let task = app
        .patch(
            &session,
            &app.item_url(&id).await?,
            &item["version"],
            json!({ "kind": "task" }),
        )
        .await?;
    app.clock.advance(domain::Duration::from_millis(1));
    app.patch(
        &session,
        &app.item_url(&id).await?,
        &task.body["version"],
        json!({ "stage": "with_context" }),
    )
    .await?;

    // Act
    let history = app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={id}", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(
        history.body["entries"][0]["changes"],
        eq(&json!([{ "field": "stage", "label": "Stage", "before": "As written", "after": "With context" }]))
    );
    expect_that!(
        history.body["entries"][1]["changes"],
        eq(&json!([{ "field": "kind", "label": "Kind", "before": "Note", "after": "Task" }]))
    );
    let created: Vec<&str> = history.body["entries"][2]["changes"]
        .as_array()
        .context("no changes")?
        .iter()
        .filter_map(|change| change["field"].as_str())
        .collect();
    expect_that!(created, not(contains(eq(&"kind"))));
    expect_that!(created, not(contains(eq(&"stage"))));
    Ok(())
}
