//! Search across all heaps (`GET /me/search`).

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

/// Makes a task called `title` in `heap`.
async fn item(app: &TestApp, session: &str, heap: &str, title: &str) -> Result<Value> {
    app.new_item(session, heap, json!({ "title": title, "need": "expected" }))
        .await
}

/// Changes `item` with `body` and returns it as it is now.
async fn patch(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<Value> {
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

/// `GET /me/search` for `q`, sent as it is.
async fn search(app: &TestApp, session: &str, q: &str) -> Result<Value> {
    let response = app
        .send(
            Method::GET,
            &format!("/api/v1/me/search?q={}", q.replace('%', "%25").replace(' ', "%20")),
            Some(session),
            None,
        )
        .await?;
    anyhow::ensure!(response.status == StatusCode::OK, "search failed: {}", response.body);
    Ok(response.body)
}

/// The titles of the rows, in order.
fn titles(found: &Value) -> Vec<String> {
    found["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row["item"]["title"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn finds_a_title_ignoring_case_in_every_heap_with_its_heap_and_team() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &personal, "Rotate the certs").await?;
    item(&app, &session, &shared, "ROTATE keys").await?;
    item(&app, &session, &shared, "Unrelated").await?;

    // Act
    let found = search(&app, &session, "rotate").await?;

    // Assert
    expect_that!(
        titles(&found),
        unordered_elements_are![eq("Rotate the certs"), eq("ROTATE keys")]
    );
    expect_that!(found["truncated"], eq(&json!(false)));
    let row = found["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["item"]["title"] == "ROTATE keys"))
        .context("row")?;
    expect_that!(row["heap"]["id"], eq(&json!(shared)));
    expect_that!(row["heap"]["team_slug"], eq(&json!("shared")));
    expect_that!(row["heap"]["team_name"], eq(&json!("Shared")));
    expect_that!(row["item"]["state"], eq(&json!("open")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_with_the_key_comes_first_and_is_not_repeated_by_its_title() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let target = item(&app, &session, &heap, "Target").await?;
    let key = target["key"].as_str().context("key")?.to_owned();
    patch(&app, &session, &target, json!({ "title": format!("Target {key}") })).await?;
    app.clock.advance(Duration::from_minutes(1));
    item(&app, &session, &heap, &format!("Mentions {key}")).await?;

    // Act
    let found = search(&app, &session, &key.to_uppercase()).await?;

    // Assert
    expect_that!(
        titles(&found),
        elements_are![eq(&format!("Target {key}")), eq(&format!("Mentions {key}"))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn leaves_out_other_teams_heaps_and_archived_heaps() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    let (other, bobs_heap) = app.register("bob@example.test").await?;
    item(&app, &session, &personal, "Cert mine").await?;
    item(&app, &session, &shared, "Cert archived").await?;
    item(&app, &other, &bobs_heap, "Cert bobs").await?;
    app.set_heap_archived(&shared, true).await?;

    // Act
    let found = search(&app, &session, "cert").await?;

    // Assert
    expect_that!(titles(&found), elements_are![eq("Cert mine")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn orders_open_items_newest_first_then_closed_ones_most_recently_closed_first() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let first = item(&app, &session, &heap, "x first").await?;
    app.clock.advance(Duration::from_minutes(1));
    let second = item(&app, &session, &heap, "x second").await?;
    app.clock.advance(Duration::from_minutes(1));
    item(&app, &session, &heap, "x third").await?;
    app.clock.advance(Duration::from_minutes(1));
    patch(&app, &session, &second, json!({ "state": "done" })).await?;
    app.clock.advance(Duration::from_minutes(1));
    patch(&app, &session, &first, json!({ "state": "wont_do" })).await?;

    // Act
    let found = search(&app, &session, "x ").await?;

    // Assert
    expect_that!(
        titles(&found),
        elements_are![eq("x third"), eq("x first"), eq("x second")]
    );
    expect_that!(found["rows"][1]["item"]["state"], eq(&json!("wont_do")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn percent_and_underscore_match_only_themselves() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    item(&app, &session, &heap, "50% off").await?;
    item(&app, &session, &heap, "500 off").await?;
    item(&app, &session, &heap, "a_b").await?;
    item(&app, &session, &heap, "axb").await?;
    item(&app, &session, &heap, "wow! really").await?;

    // Act
    let percent = search(&app, &session, "50%").await?;
    let underscore = search(&app, &session, "a_b").await?;
    let bang = search(&app, &session, "wow!").await?;

    // Assert
    expect_that!(titles(&percent), elements_are![eq("50% off")]);
    expect_that!(titles(&underscore), elements_are![eq("a_b")]);
    expect_that!(titles(&bang), elements_are![eq("wow! really")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn no_match_is_an_empty_list_and_a_missing_q_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    item(&app, &session, &heap, "Something").await?;

    // Act
    let found = search(&app, &session, "nothing like it").await?;
    let missing = app.send(Method::GET, "/api/v1/me/search", Some(&session), None).await?;

    // Assert
    expect_that!(found, eq(&json!({ "rows": [], "truncated": false })));
    expect_that!(missing.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(missing.body["code"], eq(&json!("invalid_query")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_cap_holds_rows_back_and_says_so() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let user_id = me.body["user"]["id"].as_str().context("no id")?.to_owned();
    for title in ["Cert one", "Cert two", "Cert three"] {
        item(&app, &session, &heap, title).await?;
    }

    // Act
    let capped = app.search_capped(&user_id, "cert", 2).await?;
    let fits = app.search_capped(&user_id, "cert", 3).await?;

    // Assert
    expect_that!(capped, eq((2, true)));
    expect_that!(fits, eq((3, false)));
    Ok(())
}
