//! Questions and requests on an item: asking, replying, and the rules around them.

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
    let item = app
        .new_item(&session, &heap, json!({ "title": "Rotate the certs" }))
        .await?;
    let key = item["key"].as_str().context("no key")?.to_owned();
    Ok((app, session, heap, key))
}

async fn item_path(app: &TestApp, heap: &str, key: &str) -> Result<String> {
    Ok(format!("{}/items/{key}", app.heap_url(heap).await?))
}

async fn ask(app: &TestApp, session: &str, heap: &str, key: &str, body: Value) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("{}/questions", item_path(app, heap, key).await?),
        Some(session),
        Some(body),
    )
    .await
}

async fn reply(
    app: &TestApp,
    session: &str,
    heap: &str,
    key: &str,
    number: &str,
    version: &Value,
    body: Value,
) -> Result<TestResponse> {
    app.patch(
        session,
        &format!("{}/questions/{number}", item_path(app, heap, key).await?),
        version,
        body,
    )
    .await
}

async fn get(app: &TestApp, session: &str, heap: &str, key: &str) -> Result<Value> {
    let fetched = app
        .send(Method::GET, &item_path(app, heap, key).await?, Some(session), None)
        .await?;
    Ok(fetched.body)
}

fn actions(value: &Value) -> Vec<&str> {
    value["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Closes the item and returns the response.
async fn close(app: &TestApp, session: &str, heap: &str, key: &str, version: &Value) -> Result<TestResponse> {
    app.patch(
        session,
        &item_path(app, heap, key).await?,
        version,
        json!({ "state": "done" }),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn questions_are_numbered_in_order_and_leave_the_items_version_alone() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let before = get(&app, &session, &heap, &key).await?;

    // Act
    let first = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({
            "kind": "question",
            "text": " Which CA? ",
            "details": "Internal or public",
            "choices": ["Internal", "Let's Encrypt"],
        }),
    )
    .await?;
    let second = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "request", "text": "Put a staging token in the vault." }),
    )
    .await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::CREATED));
    expect_that!(first.body["questions"].as_array().map(Vec::len), some(eq(1)));
    let question = &first.body["questions"][0];
    expect_that!(question["number"], eq(&json!(1)));
    expect_that!(question["kind"], eq(&json!("question")));
    expect_that!(question["text"], eq(&json!("Which CA?")));
    expect_that!(question["details"], eq(&json!("Internal or public")));
    expect_that!(question["choices"], eq(&json!(["Internal", "Let's Encrypt"])));
    expect_that!(question["many"], eq(&json!(false)));
    expect_that!(question["step"], eq(&Value::Null));
    expect_that!(question["state"], eq(&json!("open")));
    expect_that!(question["asked_by"]["display_name"], eq(&json!("Tester")));
    expect_that!(question["asked_label"], eq(&json!("today")));
    expect_that!(question["reply"], eq(&Value::Null));
    expect_that!(question["version"], eq(&json!(1)));
    expect_that!(question["allowed_actions"], eq(&json!(["reply"])));
    let request = &second.body["questions"][1];
    expect_that!(request["number"], eq(&json!(2)));
    expect_that!(request["kind"], eq(&json!("request")));
    expect_that!(request["choices"], eq(&json!(["Done", "Can't be done"])));
    expect_that!(request["details"], eq(&Value::Null));
    expect_that!(second.body["version"], eq(&before["version"]));
    expect_that!(actions(&second.body), contains(eq(&"ask_question")));
    expect_that!(before["questions"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn list_rows_carry_no_questions() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Why?" }),
    )
    .await?;

    // Act
    let listed = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(listed.status, eq(StatusCode::OK));
    let rows = listed.body["items"]
        .as_array()
        .or_else(|| listed.body["ranked"].as_array())
        .context("no rows")?;
    expect_that!(rows.is_empty(), eq(false));
    for row in rows {
        expect_that!(row["item"]["questions"], eq(&Value::Null));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_question_names_a_step_of_the_item_even_a_dropped_one() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let item = get(&app, &session, &heap, &key).await?;
    let item_url = item_path(&app, &heap, &key).await?;
    app.patch(&session, &item_url, &item["version"], json!({ "kind": "task" }))
        .await?;
    for title in ["One", "Two"] {
        app.send(
            Method::POST,
            &format!("{item_url}/steps"),
            Some(&session),
            Some(json!({ "title": title })),
        )
        .await?;
    }
    app.patch(
        &session,
        &format!("{item_url}/steps/2"),
        &json!(1),
        json!({ "state": "dropped" }),
    )
    .await?;

    // Act
    let asked = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "About the dropped one", "step": 2 }),
    )
    .await?;
    let unknown = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "About nothing", "step": 3 }),
    )
    .await?;
    let zero = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "About zero", "step": 0 }),
    )
    .await?;

    // Assert
    expect_that!(asked.status, eq(StatusCode::CREATED));
    expect_that!(asked.body["questions"][0]["step"], eq(&json!(2)));
    expect_that!(unknown.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(unknown.body["code"], eq(&json!("value_unknown")));
    expect_that!(unknown.body["field"], eq(&json!("step")));
    expect_that!(zero.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(zero.body["field"], eq(&json!("step")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn invalid_questions_are_refused_naming_the_field() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let cases = [
        (json!({ "kind": "question", "text": "  " }), "text", "field_empty"),
        (
            json!({ "kind": "question", "text": "x".repeat(301) }),
            "text",
            "field_too_long",
        ),
        (
            json!({ "kind": "question", "text": "one\ntwo" }),
            "text",
            "field_not_single_line",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "details": "d".repeat(20_001) }),
            "details",
            "field_too_long",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "choices": ["Only"] }),
            "choices",
            "choice_count_invalid",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "choices": (0..11).map(|n| format!("c{n}")).collect::<Vec<_>>() }),
            "choices",
            "choice_count_invalid",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "choices": ["Yes", "yes"] }),
            "choices",
            "choice_duplicate",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "choices": ["Yes", "a\nb"] }),
            "choices",
            "field_not_single_line",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "choices": ["Yes", "x".repeat(101)] }),
            "choices",
            "field_too_long",
        ),
        (
            json!({ "kind": "question", "text": "Which?", "many": true }),
            "many",
            "field_not_allowed",
        ),
        (
            json!({ "kind": "request", "text": "Do it", "choices": ["A", "B"] }),
            "choices",
            "field_not_allowed",
        ),
        (
            json!({ "kind": "request", "text": "Do it", "many": true }),
            "many",
            "field_not_allowed",
        ),
        (json!({ "kind": "review", "text": "Look" }), "kind", "value_unknown"),
    ];

    // Act and assert
    for (body, field, code) in cases {
        let refused = ask(&app, &session, &heap, &key, body.clone()).await?;
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY), "{body}");
        expect_that!(refused.body["field"], eq(&json!(field)), "{body}");
        expect_that!(refused.body["code"], eq(&json!(code)), "{body}");
    }
    let fine = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "x".repeat(300) }),
    )
    .await?;
    expect_that!(fine.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_question_without_choices_is_replied_to_with_a_comment() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let before = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Which CA?" }),
    )
    .await?;

    // Act
    let blank = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "comment": "  " } }),
    )
    .await?;
    let picked = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [0], "comment": "x" } }),
    )
    .await?;
    let answered = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "comment": " The internal one " } }),
    )
    .await?;

    // Assert
    expect_that!(blank.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(blank.body["field"], eq(&json!("comment")));
    expect_that!(picked.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(picked.body["field"], eq(&json!("picked")));
    expect_that!(answered.status, eq(StatusCode::OK));
    let question = &answered.body["questions"][0];
    expect_that!(question["state"], eq(&json!("answered")));
    expect_that!(question["reply"]["picked"], eq(&json!([])));
    expect_that!(question["reply"]["comment"], eq(&json!("The internal one")));
    expect_that!(question["reply"]["replied_by"]["display_name"], eq(&json!("Tester")));
    expect_that!(question["reply"]["replied_label"], eq(&json!("today")));
    expect_that!(question["version"], eq(&json!(2)));
    expect_that!(question["allowed_actions"], eq(&json!([])));
    expect_that!(answered.body["version"], eq(&before.body["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_question_with_choices_is_replied_to_by_picking() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "One?", "choices": ["A", "B"] }),
    )
    .await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Any?", "choices": ["A", "B", "C"], "many": true }),
    )
    .await?;

    // Act
    let none = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [] } }),
    )
    .await?;
    let two = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [0, 1] } }),
    )
    .await?;
    let out_of_range = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [2] } }),
    )
    .await?;
    let repeated = reply(
        &app,
        &session,
        &heap,
        &key,
        "2",
        &json!(1),
        json!({ "reply": { "picked": [0, 0] } }),
    )
    .await?;
    let one = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [1], "comment": "Because" } }),
    )
    .await?;
    let all = reply(
        &app,
        &session,
        &heap,
        &key,
        "2",
        &json!(1),
        json!({ "reply": { "picked": [2, 0, 1] } }),
    )
    .await?;

    // Assert
    for refused in [&none, &two, &out_of_range, &repeated] {
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(refused.body["field"], eq(&json!("picked")));
    }
    expect_that!(none.body["code"], eq(&json!("pick_required")));
    expect_that!(two.body["code"], eq(&json!("pick_too_many")));
    expect_that!(out_of_range.body["code"], eq(&json!("pick_out_of_range")));
    expect_that!(repeated.body["code"], eq(&json!("pick_repeated")));
    expect_that!(one.status, eq(StatusCode::OK));
    expect_that!(one.body["questions"][0]["reply"]["picked"], eq(&json!([1])));
    expect_that!(one.body["questions"][0]["reply"]["comment"], eq(&json!("Because")));
    expect_that!(all.status, eq(StatusCode::OK));
    expect_that!(all.body["questions"][1]["reply"]["picked"], eq(&json!([0, 1, 2])));
    expect_that!(all.body["questions"][1]["reply"]["comment"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_request_is_replied_to_with_done_or_cant_be_done() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "request", "text": "Do it" }),
    )
    .await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "request", "text": "Do that" }),
    )
    .await?;

    // Act
    let empty = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [] } }),
    )
    .await?;
    let done = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [0], "comment": "Token is in the vault." } }),
    )
    .await?;
    let cant = reply(
        &app,
        &session,
        &heap,
        &key,
        "2",
        &json!(1),
        json!({ "reply": { "picked": [1] } }),
    )
    .await?;

    // Assert
    expect_that!(empty.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(empty.body["field"], eq(&json!("picked")));
    expect_that!(done.status, eq(StatusCode::OK));
    expect_that!(done.body["questions"][0]["reply"]["picked"], eq(&json!([0])));
    expect_that!(cant.body["questions"][1]["reply"]["picked"], eq(&json!([1])));
    expect_that!(cant.body["questions"][1]["reply"]["comment"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_body_without_a_reply_is_refused() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Why?" }),
    )
    .await?;

    // Act
    let refused = reply(&app, &session, &heap, &key, "1", &json!(1), json!({})).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(refused.body["field"], eq(&json!("reply")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_second_reply_is_refused_and_so_is_a_stale_version() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Why?" }),
    )
    .await?;
    let body = json!({ "reply": { "comment": "Because" } });

    // Act
    let stale = reply(&app, &session, &heap, &key, "1", &json!(7), body.clone()).await?;
    let first = reply(&app, &session, &heap, &key, "1", &json!(1), body.clone()).await?;
    let again = reply(&app, &session, &heap, &key, "1", &json!(2), body.clone()).await?;
    let late = reply(&app, &session, &heap, &key, "1", &json!(1), body).await?;
    let missing = reply(
        &app,
        &session,
        &heap,
        &key,
        "2",
        &json!(1),
        json!({ "reply": { "comment": "x" } }),
    )
    .await?;
    let nonsense = reply(
        &app,
        &session,
        &heap,
        &key,
        "abc",
        &json!(1),
        json!({ "reply": { "comment": "x" } }),
    )
    .await?;

    // Assert
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(again.body["code"], eq(&json!("question_answered")));
    expect_that!(late.body["code"], eq(&json!("question_answered")));
    expect_that!(missing.status, eq(StatusCode::NOT_FOUND));
    expect_that!(missing.body["code"], eq(&json!("question_not_in_item")));
    expect_that!(nonsense.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_keeps_its_open_questions_read_only_until_it_is_reopened() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let asked = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Why?" }),
    )
    .await?;
    let closed = close(&app, &session, &heap, &key, &asked.body["version"]).await?;

    // Act
    let asking = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "And?" }),
    )
    .await?;
    let replying = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "comment": "x" } }),
    )
    .await?;
    let reopened = app
        .patch(
            &session,
            &item_path(&app, &heap, &key).await?,
            &closed.body["version"],
            json!({ "state": "open" }),
        )
        .await?;
    let replied_after = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "comment": "x" } }),
    )
    .await?;

    // Assert
    expect_that!(closed.body["questions"][0]["state"], eq(&json!("open")));
    expect_that!(closed.body["questions"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(actions(&closed.body), not(contains(eq(&"ask_question"))));
    expect_that!(asking.status, eq(StatusCode::CONFLICT));
    expect_that!(asking.body["code"], eq(&json!("item_closed")));
    expect_that!(replying.status, eq(StatusCode::CONFLICT));
    expect_that!(replying.body["code"], eq(&json!("item_closed")));
    expect_that!(reopened.body["questions"][0]["allowed_actions"], eq(&json!(["reply"])));
    expect_that!(replied_after.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_refuses_question_writes_and_offers_nothing() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Why?" }),
    )
    .await?;
    app.set_heap_archived(&heap, true).await?;

    // Act
    let asking = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "And?" }),
    )
    .await?;
    let replying = reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "comment": "x" } }),
    )
    .await?;
    let read = get(&app, &session, &heap, &key).await?;

    // Assert
    expect_that!(asking.status, eq(StatusCode::CONFLICT));
    expect_that!(asking.body["code"], eq(&json!("heap_archived")));
    expect_that!(replying.status, eq(StatusCode::CONFLICT));
    expect_that!(replying.body["code"], eq(&json!("heap_archived")));
    expect_that!(read["questions"][0]["allowed_actions"], eq(&json!([])));
    expect_that!(actions(&read), not(contains(eq(&"ask_question"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn someone_outside_the_team_finds_nothing() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Why?" }),
    )
    .await?;
    let (stranger, _) = app.register("eve@example.test").await?;

    // Act
    let asking = ask(
        &app,
        &stranger,
        &heap,
        &key,
        json!({ "kind": "question", "text": "Spy" }),
    )
    .await?;
    let replying = reply(
        &app,
        &stranger,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "comment": "x" } }),
    )
    .await?;

    // Assert
    expect_that!(asking.status, eq(StatusCode::NOT_FOUND));
    expect_that!(replying.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_who_is_not_an_admin_can_ask_and_reply() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, _) = app.register("admin@example.test").await?;
    let (member, _) = app.register("member@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let member_id = me.body["user"]["id"].as_str().context("no user id")?.to_owned();
    app.add_member(&team, &member_id, Role::Member).await?;
    let item = app.new_item(&admin, &heap, json!({ "title": "Item" })).await?;
    let key = item["key"].as_str().context("no key")?;

    // Act
    let asked = ask(
        &app,
        &member,
        &heap,
        key,
        json!({ "kind": "request", "text": "Please" }),
    )
    .await?;
    let replied = reply(
        &app,
        &member,
        &heap,
        key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [0] } }),
    )
    .await?;

    // Assert
    expect_that!(asked.status, eq(StatusCode::CREATED));
    expect_that!(replied.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_has_at_most_one_hundred_questions() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    for n in 1..=100 {
        let asked = ask(
            &app,
            &session,
            &heap,
            &key,
            json!({ "kind": "question", "text": format!("Question {n}") }),
        )
        .await?;
        anyhow::ensure!(asked.status == StatusCode::CREATED, "question {n}: {}", asked.body);
    }

    // Act
    let one_more = ask(
        &app,
        &session,
        &heap,
        &key,
        json!({ "kind": "question", "text": "One more" }),
    )
    .await?;

    // Assert
    expect_that!(one_more.status, eq(StatusCode::CONFLICT));
    expect_that!(one_more.body["code"], eq(&json!("too_many_questions")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_items_history_describes_the_questions_asked_and_answered() -> Result<()> {
    // Arrange
    let (app, session, heap, key) = note().await?;
    let item = get(&app, &session, &heap, &key).await?;
    let id = item["id"].as_str().context("no id")?;
    app.clock.advance(domain::Duration::from_millis(1));
    ask(
        &app,
        &session,
        &heap,
        &key,
        json!({
            "kind": "question",
            "text": "Which CA?",
            "choices": ["Internal", "Let's Encrypt"],
            "many": true,
        }),
    )
    .await?;
    app.clock.advance(domain::Duration::from_millis(1));
    reply(
        &app,
        &session,
        &heap,
        &key,
        "1",
        &json!(1),
        json!({ "reply": { "picked": [0, 1], "comment": "Both" } }),
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
    let entries = app.audit_entries(id).await?;

    // Assert
    let answered = &history.body["entries"][0];
    expect_that!(answered["phrase"], eq(&json!("answered a question on")));
    expect_that!(
        answered["changes"],
        eq(&json!([
            { "field": "question", "label": "Question", "before": null, "after": "q1" },
            { "field": "question_text", "label": "Question text", "before": null, "after": "Which CA?" },
            { "field": "reply", "label": "Reply", "before": null, "after": "Internal, Let's Encrypt" },
            { "field": "reply_comment", "label": "Comment", "before": null, "after": "Both" },
        ]))
    );
    let asked = &history.body["entries"][1];
    expect_that!(asked["phrase"], eq(&json!("asked a question on")));
    expect_that!(
        asked["changes"],
        eq(&json!([
            { "field": "question", "label": "Question", "before": null, "after": "q1" },
            { "field": "question_kind", "label": "Kind", "before": null, "after": "Question" },
            { "field": "question_text", "label": "Question text", "before": null, "after": "Which CA?" },
            { "field": "question_choices", "label": "Choices", "before": null, "after": "Internal, Let's Encrypt" },
            { "field": "question_many", "label": "Pick any", "before": null, "after": "yes" },
        ]))
    );
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![eq(&"item_created"), eq(&"question_asked"), eq(&"question_answered")]
    );
    expect_that!(entries[2].1["reply"], eq(&json!("Internal, Let's Encrypt")));
    expect_that!(entries[1].1["question_kind"], eq(&json!("question")));
    Ok(())
}
