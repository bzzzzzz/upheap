//! Turns: sending an item to an agent, taking it back, and the heap's setting that agents prepare new notes.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// Registers a user and makes an open note in their heap. Returns the app, the session, the heap and the item's key.
async fn note() -> Result<(TestApp, String, String, String)> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let key = add(&app, &session, &heap, "Rotate the certs").await?;
    Ok((app, session, heap, key))
}

/// Adds a note to the heap and returns its key.
async fn add(app: &TestApp, session: &str, heap: &str, title: &str) -> Result<String> {
    let item = app.new_item(session, heap, json!({ "title": title })).await?;
    Ok(item["key"].as_str().context("no key")?.to_owned())
}

async fn item_path(app: &TestApp, heap: &str, key: &str) -> Result<String> {
    Ok(format!("{}/items/{key}", app.heap_url(heap).await?))
}

async fn send(app: &TestApp, session: &str, heap: &str, key: &str, body: Value) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/turn", item_path(app, heap, key).await?),
        Some(session),
        Some(body),
    )
    .await
}

async fn take_back(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<TestResponse> {
    app.send(
        Method::DELETE,
        &format!("{}/turn", item_path(app, heap, key).await?),
        Some(session),
        None,
    )
    .await
}

async fn get(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<Value> {
    let fetched = app
        .send(Method::GET, &item_path(app, heap, key).await?, Some(session), None)
        .await?;
    Ok(fetched.body)
}

/// The row of the item with `key` in the heap's list.
async fn listed(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<Value> {
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(heap).await?),
            Some(session),
            None,
        )
        .await?;
    ["pinned", "ranked", "blocked"]
        .iter()
        .filter_map(|group| list.body[group].as_array())
        .flatten()
        .find(|row| row["item"]["key"] == json!(key))
        .map(|row| row["item"].clone())
        .context("the item isn't listed")
}

async fn patch_item(app: &TestApp, session: &str, heap: &str, key: &str, body: Value) -> Result<TestResponse> {
    let path = item_path(app, heap, key).await?;
    let version = get(app, session, heap, key).await?["version"].clone();
    app.patch(session, &path, &version, body).await
}

/// The actions of the item's audit entries, oldest first.
async fn audit_actions(app: &TestApp, item: &Value) -> Result<Vec<String>> {
    let id = item["id"].as_str().context("no id")?;
    Ok(app
        .audit_entries(id)
        .await?
        .into_iter()
        .map(|(action, _)| action)
        .collect())
}

fn actions(value: &Value) -> Vec<&str> {
    value["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Turns the heap's `agents_prepare` setting to `on` as the heap's admin and returns the response.
async fn set_prepare(app: &TestApp, session: &str, heap: &str, on: bool) -> Result<TestResponse> {
    let path = app.heap_url(heap).await?;
    let current = app.send(Method::GET, &path, Some(session), None).await?;
    app.patch(
        session,
        &path,
        &current.body["version"],
        json!({ "agents_prepare": on }),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn sending_opens_a_turn_and_leaves_the_items_version_alone() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let before = get(&app, &session, &heap, &key).await?;

    // Act
    let sent = send(
        &app,
        &session,
        &heap,
        &key,
        json!({ "reach": "all_the_way", "line": "  Use the new CA.  " }),
    )
    .await?;
    let read = get(&app, &session, &heap, &key).await?;
    let entries = app.audit_entries(before["id"].as_str().context("no id")?).await?;

    // Assert
    expect_that!(sent.status, eq(StatusCode::CREATED));
    let turn = &sent.body["turn"];
    expect_that!(turn["reach"], eq(&json!("all_the_way")));
    expect_that!(turn["goal"], eq(&json!("with_context")));
    expect_that!(turn["line"], eq(&json!("Use the new CA.")));
    expect_that!(turn["source"], eq(&json!("go_ahead")));
    expect_that!(turn["opened_by"]["display_name"], eq(&json!("Tester")));
    expect_that!(turn["opened_at"].is_i64(), eq(true));
    expect_that!(
        turn["opened_label"].as_str().is_some_and(|label| label.contains(':')),
        eq(true)
    );
    expect_that!(before["next_goal"], eq(&json!("with_context")));
    expect_that!(sent.body["with_agent"], eq(&json!(true)));
    expect_that!(sent.body["version"], eq(&before["version"]));
    expect_that!(sent.body["waits_for_you"], eq(&json!(false)));
    expect_that!(actions(&sent.body), contains(eq(&"take_back")));
    expect_that!(actions(&sent.body), not(contains(eq(&"send_to_agent"))));
    expect_that!(before["turn"], eq(&Value::Null));
    expect_that!(before["with_agent"], eq(&json!(false)));
    expect_that!(actions(&before), contains(eq(&"send_to_agent")));
    expect_that!(read["turn"], eq(&sent.body["turn"]));
    let last = entries.last().context("no entries")?;
    expect_that!(last.0, eq("turn_opened"));
    expect_that!(
        last.1,
        eq(&json!({
            "reach": "all_the_way",
            "goal": "with_context",
            "line": "Use the new CA.",
            "source": "go_ahead",
        }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_an_agent_has_cannot_be_sent_again() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;

    // Act
    let again = send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;
    let read = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(again.body["code"], eq(&json!("turn_open")));
    expect_that!(
        audit_actions(&app, &read).await?,
        elements_are![eq("item_created"), eq("turn_opened")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_and_an_archived_heap_refuse_a_send() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let other = add(&app, &session, &heap, "Another").await?;
    patch_item(&app, &session, &heap, &key, json!({ "state": "done" })).await?;
    app.set_heap_archived(&heap, true).await?;
    let in_archived = send(&app, &session, &heap, &other, json!({ "reach": "one" })).await?;
    app.set_heap_archived(&heap, false).await?;

    // Act
    let closed = send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;

    // Assert
    expect_that!(closed.status, eq(StatusCode::CONFLICT));
    expect_that!(closed.body["code"], eq(&json!("item_closed")));
    expect_that!(in_archived.status, eq(StatusCode::CONFLICT));
    expect_that!(in_archived.body["code"], eq(&json!("heap_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_line_is_bounded_and_blank_means_none() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;

    // Act
    let too_long = send(
        &app,
        &session,
        &heap,
        &key,
        json!({ "reach": "one", "line": "x".repeat(2_001) }),
    )
    .await?;
    let unknown_reach = send(&app, &session, &heap, &key, json!({ "reach": "far" })).await?;
    let unknown_field = send(&app, &session, &heap, &key, json!({ "reach": "one", "stage": "done" })).await?;
    let at_the_limit = send(
        &app,
        &session,
        &heap,
        &key,
        json!({ "reach": "one", "line": format!("{}\nsecond line", "x".repeat(1_900)) }),
    )
    .await?;
    take_back(&app, &session, &heap, &key).await?;
    let blank = send(&app, &session, &heap, &key, json!({ "reach": "one", "line": " \n " })).await?;

    // Assert
    expect_that!(too_long.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(too_long.body["field"], eq(&json!("line")));
    expect_that!(unknown_reach.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(unknown_reach.body["field"], eq(&json!("reach")));
    expect_that!(unknown_field.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(at_the_limit.status, eq(StatusCode::CREATED));
    expect_that!(
        at_the_limit.body["turn"]["line"]
            .as_str()
            .map(|line| line.ends_with("\nsecond line")),
        some(eq(true))
    );
    expect_that!(blank.status, eq(StatusCode::CREATED));
    expect_that!(blank.body["turn"]["line"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn taking_back_ends_the_turn_and_a_second_take_back_is_a_no_op() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;
    let before = get(&app, &session, &heap, &key).await?;

    // Act
    let taken = take_back(&app, &session, &heap, &key).await?;
    let again = take_back(&app, &session, &heap, &key).await?;
    let entries = app.audit_entries(before["id"].as_str().context("no id")?).await?;

    // Assert
    expect_that!(taken.status, eq(StatusCode::OK));
    expect_that!(taken.body["turn"], eq(&Value::Null));
    expect_that!(taken.body["with_agent"], eq(&json!(false)));
    expect_that!(taken.body["version"], eq(&before["version"]));
    expect_that!(actions(&taken.body), contains(eq(&"send_to_agent")));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body, eq(&taken.body));
    let kinds: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        kinds,
        elements_are![eq(&"item_created"), eq(&"turn_opened"), eq(&"turn_taken_back")]
    );
    expect_that!(entries[2].1, eq(&json!({ "goal": "with_context", "why": "take_back" })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn starting_and_closing_take_the_turn_back_but_blocking_and_editing_do_not() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let id = get(&app, &session, &heap, &key).await?["id"]
        .as_str()
        .context("no id")?
        .to_owned();
    send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;

    // Act
    let blocked = patch_item(&app, &session, &heap, &key, json!({ "state": "blocked" })).await?;
    let unblocked = patch_item(&app, &session, &heap, &key, json!({ "state": "open" })).await?;
    let renamed = patch_item(&app, &session, &heap, &key, json!({ "title": "Rotate all the certs" })).await?;
    let started = patch_item(&app, &session, &heap, &key, json!({ "state": "in_progress" })).await?;
    send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;
    let closed = patch_item(&app, &session, &heap, &key, json!({ "state": "done" })).await?;
    let entries = app.audit_entries(&id).await?;

    // Assert
    expect_that!(blocked.body["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(unblocked.body["with_agent"], eq(&json!(true)));
    expect_that!(renamed.body["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(started.body["turn"], eq(&Value::Null));
    expect_that!(closed.body["turn"], eq(&Value::Null));
    expect_that!(closed.body["with_agent"], eq(&json!(false)));
    let kinds: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        kinds,
        elements_are![
            eq(&"item_created"),
            eq(&"turn_opened"),
            eq(&"item_updated"),
            eq(&"item_updated"),
            eq(&"item_updated"),
            eq(&"turn_taken_back"),
            eq(&"item_updated"),
            eq(&"turn_opened"),
            eq(&"turn_taken_back"),
            eq(&"item_updated"),
        ]
    );
    expect_that!(entries[5].1["why"], eq(&json!("started")));
    expect_that!(entries[8].1["why"], eq(&json!("closed")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reopening_finds_no_turn() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;
    patch_item(&app, &session, &heap, &key, json!({ "state": "done" })).await?;

    // Act
    let reopened = patch_item(&app, &session, &heap, &key, json!({ "state": "open" })).await?;

    // Assert
    expect_that!(reopened.status, eq(StatusCode::OK));
    expect_that!(reopened.body["turn"], eq(&Value::Null));
    expect_that!(reopened.body["with_agent"], eq(&json!(false)));
    expect_that!(actions(&reopened.body), contains(eq(&"send_to_agent")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_admin_sets_that_agents_prepare_and_a_member_may_not() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register("admin@example.test").await?;
    let (member, _) = app.register("member@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let member_id = me.body["user"]["id"].as_str().context("no user id")?.to_owned();
    app.add_member(&team, &member_id, Role::Member).await?;
    let before = add(&app, &admin, &heap, "Before").await?;

    // Act
    let refused = set_prepare(&app, &member, &heap, true).await?;
    let on = set_prepare(&app, &admin, &heap, true).await?;
    let after = add(&app, &member, &heap, "After").await?;
    let off = set_prepare(&app, &admin, &heap, false).await?;
    let later = add(&app, &admin, &heap, "Later").await?;
    let unchanged = set_prepare(&app, &admin, &heap, false).await?;
    let read_member = app
        .send(Method::GET, &app.heap_url(&heap).await?, Some(&member), None)
        .await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(on.status, eq(StatusCode::OK));
    expect_that!(on.body["agents_prepare"], eq(&json!(true)));
    expect_that!(actions(&on.body), contains(eq(&"set_agents_prepare")));
    expect_that!(read_member.body["agents_prepare"], eq(&json!(false)));
    expect_that!(actions(&read_member.body), not(contains(eq(&"set_agents_prepare"))));
    expect_that!(off.body["agents_prepare"], eq(&json!(false)));
    expect_that!(unchanged.body["version"], eq(&off.body["version"]));

    let before_item = get(&app, &admin, &heap, &before).await?;
    expect_that!(before_item["turn"], eq(&Value::Null));

    let prepared = get(&app, &admin, &heap, &after).await?;
    expect_that!(prepared["turn"]["source"], eq(&json!("prepare")));
    expect_that!(prepared["turn"]["reach"], eq(&json!("one")));
    expect_that!(prepared["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(prepared["turn"]["opened_by"]["display_name"], eq(&json!("Tester")));
    expect_that!(prepared["with_agent"], eq(&json!(true)));
    let entries = app.audit_entries(prepared["id"].as_str().context("no id")?).await?;
    let kinds: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(kinds, elements_are![eq(&"item_created"), eq(&"turn_opened")]);
    expect_that!(
        entries[1].1,
        eq(&json!({ "reach": "one", "goal": "with_context", "line": null, "source": "prepare" }))
    );

    // Turning it off leaves the turn the setting made, and a note added after gets none.
    let still = get(&app, &admin, &heap, &after).await?;
    expect_that!(still["turn"]["source"], eq(&json!("prepare")));
    let later_item = get(&app, &admin, &heap, &later).await?;
    expect_that!(later_item["turn"], eq(&Value::Null));

    let heap_history = app
        .send(
            Method::GET,
            &format!("{}/audit", app.heap_url(&heap).await?),
            Some(&admin),
            None,
        )
        .await?;
    let updated = heap_history.body["entries"]
        .as_array()
        .context("no entries")?
        .iter()
        .filter(|entry| entry["action"] == json!("heap_updated"))
        .count();
    expect_that!(updated, eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_added_in_a_prepare_heap_gets_a_turn_to_define_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, heap) = app.register("admin@example.test").await?;
    set_prepare(&app, &admin, &heap, true).await?;

    // Act
    let created = app
        .new_item(&admin, &heap, json!({ "title": "Rotate the certs", "kind": "task" }))
        .await?;
    let key = created["key"].as_str().context("no key")?;
    let prepared = get(&app, &admin, &heap, key).await?;

    // Assert
    expect_that!(prepared["kind"], eq(&json!("task")));
    expect_that!(prepared["turn"]["source"], eq(&json!("prepare")));
    expect_that!(prepared["turn"]["reach"], eq(&json!("one")));
    expect_that!(prepared["turn"]["goal"], eq(&json!("defined")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_agents_prepare_is_audited_with_before_and_after() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, heap) = app.register("admin@example.test").await?;

    // Act
    set_prepare(&app, &admin, &heap, true).await?;
    let entries = app.audit_entries(&heap).await?;

    // Assert
    let last = entries.last().context("no entries")?;
    expect_that!(last.0, eq("heap_updated"));
    expect_that!(
        last.1,
        eq(&json!({ "agents_prepare": { "before": false, "after": true } }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archiving_a_heap_takes_back_every_open_turn() -> Result<()> {
    // Arrange
    let (app, session, heap, first) = note().await?;
    let second = add(&app, &session, &heap, "Second").await?;
    let third = add(&app, &session, &heap, "Third").await?;
    send(&app, &session, &heap, &first, json!({ "reach": "one" })).await?;
    send(&app, &session, &heap, &second, json!({ "reach": "all_the_way" })).await?;
    let second_id = get(&app, &session, &heap, &second).await?["id"]
        .as_str()
        .context("no id")?
        .to_owned();

    // Act
    let archived = app
        .send(
            Method::POST,
            &format!("{}/archive", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let unarchived = app
        .send(
            Method::POST,
            &format!("{}/unarchive", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let first_read = get(&app, &session, &heap, &first).await?;
    let second_read = get(&app, &session, &heap, &second).await?;
    let third_read = get(&app, &session, &heap, &third).await?;
    let entries = app.audit_entries(&second_id).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(unarchived.status, eq(StatusCode::OK));
    expect_that!(first_read["turn"], eq(&Value::Null));
    expect_that!(second_read["turn"], eq(&Value::Null));
    expect_that!(third_read["turn"], eq(&Value::Null));
    expect_that!(second_read["with_agent"], eq(&json!(false)));
    let taken_back: Vec<&Value> = entries
        .iter()
        .filter(|(action, _)| action == "turn_taken_back")
        .map(|(_, diff)| diff)
        .collect();
    expect_that!(
        taken_back,
        elements_are![eq(&&json!({ "goal": "with_context", "why": "heap_archived" }))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn taking_back_in_an_archived_heap_is_refused() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    app.set_heap_archived(&heap, true).await?;

    // Act
    let taken = take_back(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(taken.status, eq(StatusCode::CONFLICT));
    expect_that!(taken.body["code"], eq(&json!("heap_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn list_rows_carry_the_turn_and_a_waiting_row_is_not_with_an_agent() -> Result<()> {
    // Arrange
    let (app, session, heap, with) = note().await?;
    let waiting = add(&app, &session, &heap, "Waiting").await?;
    let without = add(&app, &session, &heap, "Without").await?;
    let closed = add(&app, &session, &heap, "Closed").await?;
    send(&app, &session, &heap, &with, json!({ "reach": "one" })).await?;
    send(&app, &session, &heap, &waiting, json!({ "reach": "one" })).await?;
    app.send(
        Method::POST,
        &format!("{}/questions", item_path(&app, &heap, &waiting).await?),
        Some(&session),
        Some(json!({ "kind": "question", "text": "Which CA?" })),
    )
    .await?;
    patch_item(&app, &session, &heap, &closed, json!({ "state": "done" })).await?;

    // Act
    let with_row = listed(&app, &session, &heap, &with).await?;
    let waiting_row = listed(&app, &session, &heap, &waiting).await?;
    let without_row = listed(&app, &session, &heap, &without).await?;
    let closed_list = app
        .send(
            Method::GET,
            &format!("{}/items/closed", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    let waiting_list = app
        .send(Method::GET, "/api/v1/me/waiting", Some(&session), None)
        .await?;

    // Assert
    expect_that!(with_row["with_agent"], eq(&json!(true)));
    expect_that!(with_row["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(actions(&with_row), contains(eq(&"take_back")));
    expect_that!(waiting_row["waits_for_you"], eq(&json!(true)));
    expect_that!(waiting_row["with_agent"], eq(&json!(false)));
    expect_that!(waiting_row["turn"]["reach"], eq(&json!("one")));
    expect_that!(without_row["with_agent"], eq(&json!(false)));
    expect_that!(without_row["turn"], eq(&Value::Null));
    expect_that!(actions(&without_row), contains(eq(&"send_to_agent")));
    let closed_rows = closed_list.body["items"].as_array().context("no closed items")?;
    expect_that!(closed_rows[0]["turn"], eq(&Value::Null));
    expect_that!(closed_rows[0]["with_agent"], eq(&json!(false)));
    let rows = waiting_list.body["rows"].as_array().context("no rows")?;
    expect_that!(rows.len(), eq(1));
    expect_that!(rows[0]["item"]["with_agent"], eq(&json!(false)));
    expect_that!(rows[0]["item"]["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(
        rows[0]["item"]["turn"]["opened_by"]["display_name"],
        eq(&json!("Tester"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_goal_is_the_next_stable_state_of_the_item() -> Result<()> {
    // Arrange
    let (app, session, heap, note_key) = note().await?;
    let with_context = add(&app, &session, &heap, "With context").await?;
    patch_item(&app, &session, &heap, &with_context, json!({ "stage": "with_context" })).await?;
    let task = add(&app, &session, &heap, "A task").await?;
    patch_item(&app, &session, &heap, &task, json!({ "kind": "task" })).await?;
    let defined = add(&app, &session, &heap, "A defined task").await?;
    patch_item(
        &app,
        &session,
        &heap,
        &defined,
        json!({ "kind": "task", "stage": "defined" }),
    )
    .await?;

    // Act
    let mut goals = Vec::new();
    for key in [&note_key, &with_context, &task, &defined] {
        let sent = send(&app, &session, &heap, key, json!({ "reach": "one" })).await?;
        goals.push(sent.body["turn"]["goal"].clone());
    }

    // Assert
    expect_that!(
        goals,
        elements_are![
            eq(&json!("with_context")),
            eq(&json!("defined")),
            eq(&json!("defined")),
            eq(&json!("done")),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_with_an_open_question_may_be_sent_and_stays_waiting() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    app.send(
        Method::POST,
        &format!("{}/questions", item_path(&app, &heap, &key).await?),
        Some(&session),
        Some(json!({ "kind": "question", "text": "Which CA?" })),
    )
    .await?;

    // Act
    let waiting = send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;

    // Assert
    expect_that!(waiting.status, eq(StatusCode::CREATED));
    expect_that!(waiting.body["waits_for_you"], eq(&json!(true)));
    expect_that!(waiting.body["with_agent"], eq(&json!(false)));
    expect_that!(waiting.body["turn"].is_object(), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blocked_item_is_refused_a_send_until_it_is_unblocked() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let blocked = patch_item(&app, &session, &heap, &key, json!({ "state": "blocked" })).await?;

    // Act
    let refused = send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;
    let read = get(&app, &session, &heap, &key).await?;
    let audit = audit_actions(&app, &read).await?;
    patch_item(&app, &session, &heap, &key, json!({ "state": "open" })).await?;
    let unblocked = get(&app, &session, &heap, &key).await?;
    let sent = send(&app, &session, &heap, &key, json!({ "reach": "one" })).await?;

    // Assert
    expect_that!(actions(&blocked.body), not(contains(eq(&"send_to_agent"))));
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("turn_item_blocked")));
    expect_that!(read["turn"], eq(&Value::Null));
    expect_that!(audit, not(contains(eq("turn_opened"))));
    expect_that!(actions(&unblocked), contains(eq(&"send_to_agent")));
    expect_that!(sent.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn someone_outside_the_team_finds_nothing() -> Result<()> {
    // Arrange
    let (app, _session, heap, key) = note().await?;
    let (outsider, _) = app.register("eve@example.test").await?;
    let path = format!("{}/turn", item_path(&app, &heap, &key).await?);

    // Act
    let sent = app
        .send(Method::POST, &path, Some(&outsider), Some(json!({ "reach": "one" })))
        .await?;
    let taken = app.send(Method::DELETE, &path, Some(&outsider), None).await?;

    // Assert
    expect_that!(sent.status, eq(StatusCode::NOT_FOUND));
    expect_that!(taken.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

/// Adds a task and returns its key.
async fn add_task(app: &TestApp, session: &str, heap: &str, title: &str) -> Result<String> {
    let key = add(app, session, heap, title).await?;
    patch_item(app, session, heap, &key, json!({ "kind": "task" })).await?;
    Ok(key)
}

#[gtest]
#[tokio::test]
async fn an_item_lists_the_goals_it_may_be_sent_to() -> Result<()> {
    // Arrange
    let (app, session, heap, note_key) = note().await?;
    let task = add_task(&app, &session, &heap, "A task").await?;
    let reminder = app
        .new_item(
            &session,
            &heap,
            json!({ "title": "Renew", "kind": "reminder", "due": { "date": "2027-01-01", "firm": false } }),
        )
        .await?;

    // Act
    let note = get(&app, &session, &heap, &note_key).await?;
    let task = get(&app, &session, &heap, &task).await?;

    // Assert
    expect_that!(note["goal_choices"], eq(&json!(["with_context", "defined"])));
    expect_that!(task["goal_choices"], eq(&json!(["defined"])));
    expect_that!(reminder["goal_choices"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_as_written_may_be_sent_straight_to_defined_in_one_turn() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let before = get(&app, &session, &heap, &key).await?;

    // Act
    let sent = send(
        &app,
        &session,
        &heap,
        &key,
        json!({ "reach": "one", "goal": "defined" }),
    )
    .await?;
    let entries = app.audit_entries(before["id"].as_str().context("no id")?).await?;

    // Assert
    expect_that!(sent.status, eq(StatusCode::CREATED));
    expect_that!(sent.body["turn"]["goal"], eq(&json!("defined")));
    expect_that!(sent.body["turn"]["reach"], eq(&json!("one")));
    let last = entries.last().context("no entries")?;
    expect_that!(last.0, eq("turn_opened"));
    expect_that!(last.1["goal"], eq(&json!("defined")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn going_past_the_next_state_with_all_the_way_is_refused_and_opens_no_turn() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;

    // Act
    let refused = send(
        &app,
        &session,
        &heap,
        &key,
        json!({ "reach": "all_the_way", "goal": "defined" }),
    )
    .await?;
    let after = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(refused.body["code"], eq(&json!("field_not_allowed")));
    expect_that!(refused.body["field"], eq(&json!("goal")));
    expect_that!(after["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_default_goal_may_be_sent_with_either_reach() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let with_context = add(&app, &session, &heap, "With context").await?;
    patch_item(&app, &session, &heap, &with_context, json!({ "stage": "with_context" })).await?;

    // Act
    let one = send(
        &app,
        &session,
        &heap,
        &key,
        json!({ "reach": "one", "goal": "with_context" }),
    )
    .await?;
    let all = send(
        &app,
        &session,
        &heap,
        &with_context,
        json!({ "reach": "all_the_way", "goal": "defined" }),
    )
    .await?;

    // Assert
    expect_that!(one.status, eq(StatusCode::CREATED));
    expect_that!(one.body["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(all.status, eq(StatusCode::CREATED));
    expect_that!(all.body["turn"]["goal"], eq(&json!("defined")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_goal_the_item_cannot_go_to_is_refused_and_an_unknown_one_is_unknown() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let with_context = add(&app, &session, &heap, "With context").await?;
    patch_item(&app, &session, &heap, &with_context, json!({ "stage": "with_context" })).await?;
    let task = add_task(&app, &session, &heap, "A task").await?;

    // Act
    let done_on_note = send(&app, &session, &heap, &key, json!({ "reach": "one", "goal": "done" })).await?;
    let earlier = send(
        &app,
        &session,
        &heap,
        &with_context,
        json!({ "reach": "one", "goal": "with_context" }),
    )
    .await?;
    let done_on_task = send(&app, &session, &heap, &task, json!({ "reach": "one", "goal": "done" })).await?;
    let unknown = send(&app, &session, &heap, &key, json!({ "reach": "one", "goal": "far" })).await?;

    // Assert
    for refused in [&done_on_note, &earlier, &done_on_task] {
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(refused.body["code"], eq(&json!("field_not_allowed")));
        expect_that!(refused.body["field"], eq(&json!("goal")));
    }
    expect_that!(unknown.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(unknown.body["code"], eq(&json!("value_unknown")));
    expect_that!(unknown.body["field"], eq(&json!("goal")));
    Ok(())
}
