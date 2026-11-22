//! The closed items and the history of all the caller's heaps (`GET /me/items/closed`, `GET /me/audit`).

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

async fn get(app: &TestApp, session: &str, path: &str) -> Result<Value> {
    let response = app.send(Method::GET, path, Some(session), None).await?;
    anyhow::ensure!(response.status == StatusCode::OK, "{path} failed: {}", response.body);
    Ok(response.body)
}

/// Makes an item called `title` in `heap`, closes it as done a minute later, and returns it.
async fn closed_item(app: &TestApp, session: &str, heap: &str, title: &str) -> Result<Value> {
    let item = app.new_item(session, heap, json!({ "title": title })).await?;
    app.clock.advance(domain::Duration::from_minutes(1));
    let saved = app
        .patch(
            session,
            &app.item_url(item["id"].as_str().context("id")?).await?,
            &item["version"],
            json!({ "state": "done" }),
        )
        .await?;
    anyhow::ensure!(saved.status == StatusCode::OK, "close failed: {}", saved.body);
    Ok(saved.body)
}

/// `(heap name, item title)` of each closed row, in order.
fn closed_rows(page: &Value) -> Vec<(String, String)> {
    page["items"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        row["heap"]["name"].as_str().unwrap_or_default().to_owned(),
                        row["item"]["title"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `(heap name, action, entity label)` of each history row, in order.
fn history_rows(page: &Value) -> Vec<(String, String, String)> {
    page["entries"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        row["heap"]["name"].as_str().unwrap_or_default().to_owned(),
                        row["entry"]["action"].as_str().unwrap_or_default().to_owned(),
                        row["entry"]["entity"]["label"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn row(heap: &str, title: &str) -> (String, String) {
    (heap.to_owned(), title.to_owned())
}

#[gtest]
#[tokio::test]
async fn closed_items_of_every_heap_come_most_recently_closed_first_each_with_its_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    let personal_name = get(&app, &session, &app.heap_url(&personal).await?).await?["name"]
        .as_str()
        .context("name")?
        .to_owned();
    closed_item(&app, &session, &personal, "First").await?;
    closed_item(&app, &session, &shared, "Second").await?;
    closed_item(&app, &session, &personal, "Third").await?;
    app.new_item(&session, &shared, json!({ "title": "Still open" }))
        .await?;

    // Act
    let all = get(&app, &session, "/api/v1/me/items/closed").await?;
    let scoped = get(&app, &session, "/api/v1/me/items/closed?team=shared").await?;
    let own = get(
        &app,
        &session,
        &format!("{}/items/closed", app.heap_url(&shared).await?),
    )
    .await?;

    // Assert
    expect_that!(
        closed_rows(&all),
        eq(&vec![
            row(&personal_name, "Third"),
            row("Shared", "Second"),
            row(&personal_name, "First")
        ])
    );
    expect_that!(all["next_cursor"], eq(&Value::Null));
    expect_that!(all["items"][1]["heap"]["team_slug"], eq(&json!("shared")));
    expect_that!(closed_rows(&scoped), eq(&vec![row("Shared", "Second")]));
    // A row is the item as its own heap's closed list gives it.
    expect_that!(all["items"][1]["item"], eq(&own["items"][0]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn closed_items_page_by_cursor_across_heaps() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    for (heap, title) in [(&personal, "One"), (&shared, "Two"), (&personal, "Three")] {
        closed_item(&app, &session, heap, title).await?;
    }

    // Act
    let first = get(&app, &session, "/api/v1/me/items/closed?limit=2").await?;
    let cursor = first["next_cursor"].as_str().context("no cursor")?;
    let second = get(
        &app,
        &session,
        &format!("/api/v1/me/items/closed?limit=2&cursor={cursor}"),
    )
    .await?;
    let bad = app
        .send(Method::GET, "/api/v1/me/items/closed?cursor=nope", Some(&session), None)
        .await?;

    // Assert
    let titles = |page: &Value| -> Vec<String> { closed_rows(page).into_iter().map(|(_, title)| title).collect() };
    expect_that!(titles(&first), elements_are![eq("Three"), eq("Two")]);
    expect_that!(titles(&second), elements_are![eq("One")]);
    expect_that!(second["next_cursor"], eq(&Value::Null));
    expect_that!(bad.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(bad.body["code"], eq(&json!("invalid_cursor")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archived_heaps_and_other_peoples_heaps_have_no_closed_items_or_history_here() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    let (other, bobs_heap) = app.register("bob@example.test").await?;
    closed_item(&app, &session, &personal, "Mine").await?;
    closed_item(&app, &session, &shared, "In an archived heap").await?;
    closed_item(&app, &other, &bobs_heap, "Bob's").await?;
    app.set_heap_archived(&shared, true).await?;

    // Act
    let closed = get(&app, &session, "/api/v1/me/items/closed").await?;
    let history = get(&app, &session, "/api/v1/me/audit").await?;

    // Assert
    let titles: Vec<String> = closed_rows(&closed).into_iter().map(|(_, title)| title).collect();
    expect_that!(titles, elements_are![eq("Mine")]);
    let about: Vec<String> = history_rows(&history).into_iter().map(|(_, _, label)| label).collect();
    expect_that!(about, contains(eq("Mine")));
    for unseen in ["In an archived heap", "Bob's", "Shared"] {
        expect_that!(about, not(contains(eq(unseen))));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_the_caller_is_not_in_is_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    app.register("bob@example.test").await?;

    // Act and assert
    for path in [
        "/api/v1/me/items/closed?team=bob",
        "/api/v1/me/items/closed?team=no-such-team",
        "/api/v1/me/audit?team=bob",
        "/api/v1/me/audit?team=Not%20A%20Handle",
    ] {
        let response = app.send(Method::GET, path, Some(&session), None).await?;
        expect_that!(response.status, eq(StatusCode::NOT_FOUND));
        expect_that!(response.body["code"], eq(&json!("not_found")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn history_of_every_heap_comes_newest_first_each_entry_with_its_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    app.clock.advance(domain::Duration::from_minutes(1));
    app.new_item(&session, &personal, json!({ "title": "Rotate the certs" }))
        .await?;
    closed_item(&app, &session, &shared, "Plan the launch").await?;

    // Act
    let all = get(&app, &session, "/api/v1/me/audit?limit=2").await?;
    let cursor = all["next_cursor"].as_str().context("no cursor")?;
    let next = get(&app, &session, &format!("/api/v1/me/audit?limit=1&cursor={cursor}")).await?;
    let scoped = get(&app, &session, "/api/v1/me/audit?team=shared").await?;
    let own = get(&app, &session, &format!("{}/audit", app.heap_url(&shared).await?)).await?;

    // Assert
    let entry = |heap: &str, action: &str, label: &str| (heap.to_owned(), action.to_owned(), label.to_owned());
    expect_that!(
        history_rows(&all),
        eq(&vec![
            entry("Shared", "item_updated", "Plan the launch"),
            entry("Shared", "item_created", "Plan the launch"),
        ])
    );
    expect_that!(history_rows(&next).len(), eq(1));
    expect_that!(history_rows(&next)[0].2.as_str(), eq("Rotate the certs"));
    // With a team, the entries are its heaps' own, each as the heap's history gives it.
    let scoped_entries: Vec<&Value> = scoped["entries"]
        .as_array()
        .context("entries")?
        .iter()
        .map(|row| &row["entry"])
        .collect();
    let own_entries: Vec<&Value> = own["entries"].as_array().context("entries")?.iter().collect();
    expect_that!(scoped_entries, eq(&own_entries));
    expect_that!(scoped["entries"][0]["heap"]["team_slug"], eq(&json!("shared")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn history_across_heaps_shows_each_heaps_entries_by_the_callers_role_in_its_team() -> Result<()> {
    // Arrange: Bob is a member of Ada's team, where only an admin sees that the heap was made.
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (team, shared) = app.shared_heap(&ada).await?;
    let (bob, _) = app.register("bob@example.test").await?;
    let bob_id = get(&app, &bob, "/api/v1/me").await?["user"]["id"]
        .as_str()
        .context("bob's id")?
        .to_owned();
    app.add_member(&team, &bob_id, Role::Member).await?;
    app.clock.advance(domain::Duration::from_minutes(1));
    app.new_item(&ada, &shared, json!({ "title": "Plan the launch" }))
        .await?;

    // Act
    let for_admin = get(&app, &ada, "/api/v1/me/audit?team=shared").await?;
    let for_member = get(&app, &bob, "/api/v1/me/audit?team=shared").await?;

    // Assert
    let actions =
        |page: &Value| -> Vec<String> { history_rows(page).into_iter().map(|(_, action, _)| action).collect() };
    expect_that!(
        actions(&for_admin),
        elements_are![eq("item_created"), eq("heap_created")]
    );
    expect_that!(actions(&for_member), elements_are![eq("item_created")]);
    Ok(())
}
