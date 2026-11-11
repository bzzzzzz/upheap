//! One ranked list across all heaps (`GET /me/items`).

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};

/// Makes an item of `need` in `heap` and returns the whole item.
async fn item(app: &TestApp, session: &str, heap: &str, title: &str, need: &str) -> Result<Value> {
    app.new_item(session, heap, json!({ "title": title, "need": need }))
        .await
}

/// `GET /me/items` with `query` (like `?need=critical`, or empty).
async fn across(app: &TestApp, session: &str, query: &str) -> Result<Value> {
    let response = app
        .send(Method::GET, &format!("/api/v1/me/items{query}"), Some(session), None)
        .await?;
    anyhow::ensure!(response.status == StatusCode::OK, "list failed: {}", response.body);
    Ok(response.body)
}

/// The titles of the rows of `group`, in order.
fn titles(list: &Value, group: &str) -> Vec<String> {
    list[group]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row["entry"]["item"]["title"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Makes a heap called `name` in the team of `team_id`, and returns its ID.
async fn make_heap(app: &TestApp, session: &str, team_id: &str, name: &str) -> Result<String> {
    let team_url = app.team_url(team_id).await?;
    let made = app
        .send(
            Method::POST,
            &format!("{team_url}/heaps"),
            Some(session),
            Some(json!({ "name": name })),
        )
        .await?;
    anyhow::ensure!(made.status == StatusCode::CREATED, "heap failed: {}", made.body);
    Ok(made.body["id"].as_str().context("heap id")?.to_owned())
}

async fn patch_state(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<Value> {
    let saved = app
        .patch(
            session,
            &app.item_url(item["id"].as_str().context("id")?).await?,
            &item["version"],
            body,
        )
        .await?;
    anyhow::ensure!(saved.status == StatusCode::OK, "patch failed: {}", saved.body);
    Ok(saved.body)
}

#[gtest]
#[tokio::test]
async fn nothing_listed_is_an_empty_list() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(list["count"], eq(&json!(0)));
    expect_that!(list["pinned"], eq(&json!([])));
    expect_that!(list["ranked"], eq(&json!([])));
    expect_that!(list["blocked"], eq(&json!([])));
    expect_that!(list["hidden_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_are_ordered_by_score_across_heaps_each_with_its_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &personal, "A critical", "critical").await?;
    item(&app, &session, &personal, "A idea", "idea").await?;
    item(&app, &session, &shared, "B expected", "expected").await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(list["count"], eq(&json!(3)));
    expect_that!(
        titles(&list, "ranked"),
        elements_are![eq("A critical"), eq("B expected"), eq("A idea")]
    );
    let rows = list["ranked"].as_array().context("rows")?;
    expect_that!(rows[0]["heap"]["id"], eq(&json!(personal)));
    expect_that!(rows[1]["heap"]["id"], eq(&json!(shared)));
    expect_that!(rows[1]["heap"]["slug"], eq(&json!("shared")));
    expect_that!(rows[1]["heap"]["team_slug"], eq(&json!("shared")));
    expect_that!(rows[1]["heap"]["team_name"], eq(&json!("Shared")));
    expect_that!(
        rows.iter().map(|row| row["position"].clone()).collect::<Vec<_>>(),
        elements_are![eq(&json!(1)), eq(&json!(2)), eq(&json!(3))]
    );
    // The entry's own position is within its heap.
    expect_that!(
        rows.iter()
            .map(|row| row["entry"]["position"].clone())
            .collect::<Vec<_>>(),
        elements_are![eq(&json!(1)), eq(&json!(1)), eq(&json!(2))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn equal_scores_in_two_heaps_order_by_heap_name_whatever_order_the_heaps_were_made_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, _) = app.shared_heap(&session).await?;
    // Made in reverse, so neither creation order nor ID order is the name order.
    let beta = make_heap(&app, &session, &team, "Beta").await?;
    let alpha = make_heap(&app, &session, &team, "Alpha").await?;
    item(&app, &session, &beta, "In Beta", "expected").await?;
    item(&app, &session, &alpha, "In Alpha", "expected").await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(titles(&list, "ranked"), elements_are![eq("In Alpha"), eq("In Beta")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn in_progress_items_are_pinned_and_blocked_ones_hidden_unless_shown() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    let started = item(&app, &session, &personal, "Started", "idea").await?;
    patch_state(&app, &session, &started, json!({ "state": "in_progress" })).await?;
    let also_started = item(&app, &session, &shared, "Also started", "critical").await?;
    patch_state(&app, &session, &also_started, json!({ "state": "in_progress" })).await?;
    item(&app, &session, &personal, "Plain", "expected").await?;
    let blocker = item(&app, &session, &personal, "The blocker", "idea").await?;
    for (heap, title) in [(&personal, "Blocked here"), (&shared, "Blocked there")] {
        let waiting = item(&app, &session, heap, title, "expected").await?;
        let on = if heap == &personal {
            blocker.clone()
        } else {
            item(&app, &session, heap, "Its blocker", "idea").await?
        };
        patch_state(
            &app,
            &session,
            &waiting,
            json!({ "state": "blocked", "blocked_by_item_id": on["id"] }),
        )
        .await?;
    }

    // Act
    let by_default = across(&app, &session, "").await?;
    let shown = across(&app, &session, "?blocked=shown").await?;

    // Assert
    expect_that!(
        titles(&by_default, "pinned"),
        elements_are![eq("Also started"), eq("Started")]
    );
    expect_that!(titles(&by_default, "blocked"), is_empty());
    expect_that!(by_default["hidden_count"], eq(&json!(2)));
    expect_that!(by_default["in_progress_count"], eq(&json!(2)));
    expect_that!(
        titles(&shown, "blocked"),
        unordered_elements_are![eq("Blocked here"), eq("Blocked there")]
    );
    expect_that!(shown["hidden_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_filter_applies_to_every_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &personal, "Critical here", "critical").await?;
    item(&app, &session, &personal, "Idea here", "idea").await?;
    item(&app, &session, &shared, "Critical there", "critical").await?;
    item(&app, &session, &shared, "Expected there", "expected").await?;

    // Act
    let list = across(&app, &session, "?need=critical").await?;

    // Assert
    expect_that!(list["count"], eq(&json!(2)));
    expect_that!(
        titles(&list, "ranked"),
        unordered_elements_are![eq("Critical here"), eq("Critical there")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archived_heaps_and_other_peoples_heaps_are_left_out() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (team, _) = app.shared_heap(&session).await?;
    let (other, bobs_heap) = app.register("bob@example.test").await?;
    let archived = make_heap(&app, &session, &team, "Archived").await?;
    item(&app, &session, &personal, "Mine", "expected").await?;
    item(&app, &session, &archived, "In an archived heap", "critical").await?;
    item(&app, &other, &bobs_heap, "Bob's", "critical").await?;
    app.set_heap_archived(&archived, true).await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(titles(&list, "ranked"), elements_are![eq("Mine")]);
    expect_that!(list["count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_parameter_of_a_single_heap_list_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;

    for query in [
        "?tag=00000000-0000-0000-0000-000000000000",
        "?sort=due",
        "?sort_parameter=00000000-0000-0000-0000-000000000000",
        "?param=00000000-0000-0000-0000-000000000000",
        "?nope=1",
    ] {
        // Act
        let response = app
            .send(Method::GET, &format!("/api/v1/me/items{query}"), Some(&session), None)
            .await?;

        // Assert
        expect_that!(response.status, eq(StatusCode::BAD_REQUEST), "{query}");
        expect_that!(response.body["code"], eq(&json!("invalid_query")), "{query}");
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_count_says_how_many_are_listed_when_the_cap_holds_back_rows() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let user_id = me.body["user"]["id"].as_str().context("no id")?.to_owned();
    for title in ["First", "Second", "Third"] {
        item(&app, &session, &heap, title, "expected").await?;
    }

    // Act
    let (count, rows) = app.across_heaps_capped(&user_id, 2).await?;

    // Assert
    expect_that!(count, eq(3));
    expect_that!(rows, eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_row_matches_the_entry_its_heaps_own_list_gives() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &shared, "Critical", "critical").await?;
    item(&app, &session, &shared, "Expected", "expected").await?;
    let one_heap = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&shared).await?),
            Some(&session),
            None,
        )
        .await?;

    // Act
    let list = across(&app, &session, &format!("?as_of={}", one_heap.body["as_of"])).await?;

    // Assert
    expect_that!(list["as_of"], eq(&one_heap.body["as_of"]));
    for (row, own) in list["ranked"]
        .as_array()
        .context("rows")?
        .iter()
        .zip(one_heap.body["ranked"].as_array().context("own rows")?)
    {
        expect_that!(row["entry"], eq(own));
    }
    expect_that!(list["ranked"].as_array().map(Vec::len), some(eq(2)));
    Ok(())
}
