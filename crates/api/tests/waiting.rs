//! Waiting for me: the open questions across the caller's heaps, and `waits_for_you` on items.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

async fn item_path(app: &TestApp, heap: &str, key: &str) -> Result<String> {
    Ok(format!("{}/items/{key}", app.heap_url(heap).await?))
}

/// Makes an item of `need` in `heap` and returns its key.
async fn item(app: &TestApp, session: &str, heap: &str, title: &str, need: &str) -> Result<String> {
    let created = app
        .new_item(session, heap, json!({ "title": title, "need": need }))
        .await?;
    Ok(created["key"].as_str().context("no key")?.to_owned())
}

/// Asks a question on the item and returns the whole item.
async fn ask(app: &TestApp, session: &str, heap: &str, key: &str, body: Value) -> Result<Value> {
    let asked = app
        .send(
            Method::POST,
            &format!("{}/questions", item_path(app, heap, key).await?),
            Some(session),
            Some(body),
        )
        .await?;
    anyhow::ensure!(asked.status == StatusCode::CREATED, "ask failed: {}", asked.body);
    Ok(asked.body)
}

async fn ask_text(app: &TestApp, session: &str, heap: &str, key: &str, text: &str) -> Result<Value> {
    ask(app, session, heap, key, json!({ "kind": "question", "text": text })).await
}

/// Replies to question `number` of the item and returns the whole item.
async fn reply(app: &TestApp, session: &str, heap: &str, key: &str, number: u32, version: &Value) -> Result<Value> {
    let replied = app
        .patch(
            session,
            &format!("{}/questions/{number}", item_path(app, heap, key).await?),
            version,
            json!({ "reply": { "comment": "Fine" } }),
        )
        .await?;
    anyhow::ensure!(replied.status == StatusCode::OK, "reply failed: {}", replied.body);
    Ok(replied.body)
}

async fn get_item(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<Value> {
    let fetched = app
        .send(Method::GET, &item_path(app, heap, key).await?, Some(session), None)
        .await?;
    Ok(fetched.body)
}

async fn waiting(app: &TestApp, session: &str) -> Result<Value> {
    let response = app.send(Method::GET, "/api/v1/me/waiting", Some(session), None).await?;
    anyhow::ensure!(response.status == StatusCode::OK, "waiting failed: {}", response.body);
    Ok(response.body)
}

/// `(item title, question number)` of each row.
fn rows(waiting: &Value) -> Vec<(String, u64)> {
    waiting["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        row["item"]["title"].as_str().unwrap_or_default().to_owned(),
                        row["question"]["number"].as_u64().unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the item with `key` waits for you, as the heap's list says.
async fn listed_waits(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<bool> {
    let listed = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(heap).await?),
            Some(session),
            None,
        )
        .await?;
    ["pinned", "ranked", "blocked"]
        .iter()
        .filter_map(|group| listed.body[group].as_array())
        .flatten()
        .find(|row| row["item"]["key"] == json!(key))
        .and_then(|row| row["item"]["waits_for_you"].as_bool())
        .context("the item isn't listed")
}

#[gtest]
#[tokio::test]
async fn nothing_waiting_is_an_empty_list() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    item(&app, &session, &heap, "No questions", "expected").await?;

    // Act
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(waiting, eq(&json!({ "count": 0, "rows": [] })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn rows_are_ordered_by_score_across_heaps_and_leave_out_what_cannot_be_answered() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (shared_team, shared) = app.shared_heap(&session).await?;
    let (other, bobs_heap) = app.register("bob@example.test").await?;

    let expected = item(&app, &session, &personal, "Expected, two questions", "expected").await?;
    let idea = item(&app, &session, &personal, "An idea", "idea").await?;
    let critical = item(&app, &session, &shared, "Critical, in the shared heap", "critical").await?;
    let closed = item(&app, &session, &personal, "Closed after asking", "critical").await?;
    let bobs = item(&app, &other, &bobs_heap, "Bob's own", "critical").await?;

    ask_text(&app, &session, &personal, &expected, "First?").await?;
    let asked = ask_text(&app, &session, &personal, &expected, "Second?").await?;
    ask_text(&app, &session, &personal, &expected, "Third?").await?;
    ask_text(&app, &session, &personal, &idea, "Only?").await?;
    ask_text(&app, &session, &shared, &critical, "Urgent?").await?;
    ask_text(&app, &session, &personal, &closed, "Too late?").await?;
    ask_text(&app, &other, &bobs_heap, &bobs, "Not yours?").await?;
    // The second question is answered, so it no longer waits.
    reply(
        &app,
        &session,
        &personal,
        &expected,
        2,
        &asked["questions"][1]["version"],
    )
    .await?;
    // The closed item's question stays open but doesn't wait.
    let closing = get_item(&app, &session, &personal, &closed).await?;
    app.patch(
        &session,
        &item_path(&app, &personal, &closed).await?,
        &closing["version"],
        json!({ "state": "done" }),
    )
    .await?;
    // A heap archived with a question still open.
    let team_url = app.team_url(&shared_team).await?;
    let archived_heap = app
        .send(
            Method::POST,
            &format!("{team_url}/heaps"),
            Some(&session),
            Some(json!({ "name": "Archived" })),
        )
        .await?;
    let archived_heap = archived_heap.body["id"].as_str().context("heap id")?.to_owned();
    let archived_key = item(&app, &session, &archived_heap, "Item of an archived heap", "critical").await?;
    ask_text(&app, &session, &archived_heap, &archived_key, "Anyone?").await?;
    app.set_heap_archived(&archived_heap, true).await?;

    // Act
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(waiting["count"], eq(&json!(4)));
    expect_that!(
        rows(&waiting),
        elements_are![
            eq(&("Critical, in the shared heap".to_owned(), 1)),
            eq(&("Expected, two questions".to_owned(), 1)),
            eq(&("Expected, two questions".to_owned(), 3)),
            eq(&("An idea".to_owned(), 1)),
        ]
    );
    let first = &waiting["rows"][0];
    expect_that!(first["kind"], eq(&json!("question")));
    expect_that!(first["item"]["key"], eq(&json!(critical)));
    expect_that!(first["heap"]["name"], eq(&json!("Shared")));
    expect_that!(first["heap"]["team_name"], eq(&json!("Shared")));
    expect_that!(first["heap"]["id"], eq(&json!(shared)));
    expect_that!(first["question"]["text"], eq(&json!("Urgent?")));
    expect_that!(first["question"]["allowed_actions"], eq(&json!(["reply"])));
    expect_that!(first["step_title"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_sees_the_questions_of_their_teams_heaps() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register("ada@example.test").await?;
    let (team, shared) = app.shared_heap(&admin).await?;
    let (member, _) = app.register("cy@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let member_id = me.body["user"]["id"].as_str().context("no id")?.to_owned();
    app.add_member(&team, &member_id, Role::Member).await?;
    let key = item(&app, &admin, &shared, "Pick a CA", "expected").await?;
    ask_text(&app, &admin, &shared, &key, "Which one?").await?;

    // Act
    let seen = waiting(&app, &member).await?;
    let outsider = app.register("dee@example.test").await?.0;
    let hidden = waiting(&app, &outsider).await?;

    // Assert
    expect_that!(rows(&seen), elements_are![eq(&("Pick a CA".to_owned(), 1))]);
    expect_that!(
        seen["rows"][0]["question"]["asked_by"]["display_name"],
        eq(&json!("Tester"))
    );
    expect_that!(hidden["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_row_carries_the_title_of_the_step_its_question_names() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let key = item(&app, &session, &heap, "Release", "expected").await?;
    let item_url = item_path(&app, &heap, &key).await?;
    let note = get_item(&app, &session, &heap, &key).await?;
    app.patch(&session, &item_url, &note["version"], json!({ "kind": "task" }))
        .await?;
    app.send(
        Method::POST,
        &format!("{item_url}/steps"),
        Some(&session),
        Some(json!({ "title": "Run in staging" })),
    )
    .await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "request", "text": "Put a token in the vault.", "step": 1 }),
    )
    .await?;
    ask_text(&app, &session, &heap, &key, "About the whole thing?").await?;

    // Act
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(waiting["count"], eq(&json!(2)));
    expect_that!(waiting["rows"][0]["step_title"], eq(&json!("Run in staging")));
    expect_that!(waiting["rows"][0]["question"]["step"], eq(&json!(1)));
    expect_that!(waiting["rows"][1]["step_title"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_waits_for_you_while_it_is_open_with_an_open_question_in_an_active_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let key = item(&app, &session, &heap, "Rotate the certs", "expected").await?;
    let quiet = item(&app, &session, &heap, "Nothing to ask", "expected").await?;
    let before = get_item(&app, &session, &heap, &key).await?;

    // Act
    let asked = ask_text(&app, &session, &heap, &key, "Which CA?").await?;
    let waits_in_list = listed_waits(&app, &session, &heap, &key).await?;
    let quiet_waits_in_list = listed_waits(&app, &session, &heap, &quiet).await?;
    let read = get_item(&app, &session, &heap, &key).await?;
    let replied = reply(&app, &session, &heap, &key, 1, &asked["questions"][0]["version"]).await?;
    let waits_after_reply = listed_waits(&app, &session, &heap, &key).await?;
    let asked_again = ask_text(&app, &session, &heap, &key, "And then?").await?;
    let closed = app
        .patch(
            &session,
            &item_path(&app, &heap, &key).await?,
            &asked_again["version"],
            json!({ "state": "done" }),
        )
        .await?;
    let closed_list = app
        .send(
            Method::GET,
            &format!("{}/items/closed", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;
    app.set_heap_archived(&heap, true).await?;
    let archived_read = get_item(&app, &session, &heap, &quiet).await?;

    // Assert
    expect_that!(before["waits_for_you"], eq(&json!(false)));
    expect_that!(asked["waits_for_you"], eq(&json!(true)));
    expect_that!(waits_in_list, eq(true));
    expect_that!(quiet_waits_in_list, eq(false));
    expect_that!(read["waits_for_you"], eq(&json!(true)));
    expect_that!(replied["waits_for_you"], eq(&json!(false)));
    expect_that!(waits_after_reply, eq(false));
    expect_that!(asked_again["waits_for_you"], eq(&json!(true)));
    expect_that!(closed.body["waits_for_you"], eq(&json!(false)));
    let closed_rows = closed_list.body["items"].as_array().context("no closed items")?;
    expect_that!(closed_rows.len(), eq(1));
    expect_that!(closed_rows[0]["waits_for_you"], eq(&json!(false)));
    expect_that!(archived_read["waits_for_you"], eq(&json!(false)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_in_an_archived_heap_does_not_wait_in_the_list_or_on_its_own() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let key = item(&app, &session, &heap, "Rotate the certs", "expected").await?;
    ask_text(&app, &session, &heap, &key, "Which CA?").await?;
    app.set_heap_archived(&heap, true).await?;

    // Act
    let in_list = listed_waits(&app, &session, &heap, &key).await?;
    let read = get_item(&app, &session, &heap, &key).await?;
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(in_list, eq(false));
    expect_that!(read["waits_for_you"], eq(&json!(false)));
    expect_that!(waiting["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_count_says_how_many_wait_when_the_cap_holds_back_rows() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let user_id = me.body["user"]["id"].as_str().context("no id")?.to_owned();
    for title in ["First", "Second", "Third"] {
        let key = item(&app, &session, &heap, title, "expected").await?;
        ask_text(&app, &session, &heap, &key, "Which?").await?;
    }

    // Act
    let (count, rows) = app.waiting_capped(&user_id, 2).await?;
    let uncapped = waiting(&app, &session).await?;

    // Assert
    expect_that!(count, eq(3));
    expect_that!(rows, eq(2));
    expect_that!(uncapped["count"], eq(&json!(3)));
    expect_that!(uncapped["rows"].as_array().map(Vec::len), some(eq(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn equal_scores_in_two_heaps_order_by_heap_name_whatever_order_the_heaps_were_made_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, _) = app.shared_heap(&session).await?;
    let team_url = app.team_url(&team).await?;
    let mut heaps = Vec::new();
    // Made in reverse, so neither creation order nor ID order is the name order.
    for name in ["Beta", "Alpha"] {
        let made = app
            .send(
                Method::POST,
                &format!("{team_url}/heaps"),
                Some(&session),
                Some(json!({ "name": name })),
            )
            .await?;
        heaps.push(made.body["id"].as_str().context("heap id")?.to_owned());
    }
    for (heap, title) in heaps.iter().zip(["In Beta", "In Alpha"]) {
        let key = item(&app, &session, heap, title, "expected").await?;
        ask_text(&app, &session, heap, &key, "Which?").await?;
    }

    // Act
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(
        rows(&waiting),
        elements_are![eq(&("In Alpha".to_owned(), 1)), eq(&("In Beta".to_owned(), 1))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_waiting_blocker_ranks_with_what_it_blocks() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let blocker = item(&app, &session, &heap, "Quiet blocker", "idea").await?;
    let blocked = item(&app, &session, &heap, "Critical, waiting on it", "critical").await?;
    let plain = item(&app, &session, &heap, "Plain expected", "expected").await?;
    ask_text(&app, &session, &heap, &blocker, "Can you?").await?;
    ask_text(&app, &session, &heap, &plain, "Which?").await?;
    let blocker_item = get_item(&app, &session, &heap, &blocker).await?;
    let blocked_item = get_item(&app, &session, &heap, &blocked).await?;
    let saved = app
        .patch(
            &session,
            &item_path(&app, &heap, &blocked).await?,
            &blocked_item["version"],
            json!({ "state": "blocked", "blocked_by_item_id": blocker_item["id"] }),
        )
        .await?;
    anyhow::ensure!(saved.status == StatusCode::OK, "block failed: {}", saved.body);

    // Act
    let waiting = waiting(&app, &session).await?;

    // Assert
    expect_that!(
        rows(&waiting),
        elements_are![
            eq(&("Quiet blocker".to_owned(), 1)),
            eq(&("Plain expected".to_owned(), 1))
        ]
    );
    Ok(())
}
