//! The list view parameters of `GET /heaps/{id}/items`: which states are pinned, shown or hidden, the filters and
//! the sort. Effort sets the priority order in these tests: with nothing else set, smaller effort ranks higher.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

/// Adds an item and returns its ID.
async fn add(app: &TestApp, session: &str, heap: &str, body: Value) -> Result<String> {
    let item = app.new_item(session, heap, body).await?;
    Ok(item["id"].as_str().unwrap_or_default().to_owned())
}

/// Moves a new item (version 1) to `state`.
async fn move_to(app: &TestApp, session: &str, item: &str, state: &str) -> Result<()> {
    let moved = app
        .patch(
            session,
            &app.item_url(&item).await?,
            &json!(1),
            json!({ "state": state }),
        )
        .await?;
    anyhow::ensure!(moved.status == StatusCode::OK, "move failed: {}", moved.body);
    Ok(())
}

async fn list(app: &TestApp, session: &str, heap: &str, query: &str) -> Result<TestResponse> {
    app.send(
        Method::GET,
        &format!("{}/items?{query}", app.heap_url(&heap).await?),
        Some(session),
        None,
    )
    .await
}

/// The titles of one group of the list.
fn titles(group: &Value) -> Vec<String> {
    group
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| entry["item"]["title"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A heap with an open, an in-progress and a blocked item.
async fn one_of_each_state(app: &TestApp, session: &str, heap: &str) -> Result<()> {
    add(app, session, heap, json!({ "title": "Open", "effort": "m" })).await?;
    let in_progress = add(app, session, heap, json!({ "title": "In progress" })).await?;
    move_to(app, session, &in_progress, "in_progress").await?;
    let blocked = add(app, session, heap, json!({ "title": "Blocked", "effort": "s" })).await?;
    move_to(app, session, &blocked, "blocked").await?;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn by_default_in_progress_is_pinned_and_blocked_is_hidden_but_counted() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    one_of_each_state(&app, &session, &heap).await?;

    // Act
    let response = list(&app, &session, &heap, "").await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::OK));
    expect_that!(titles(&response.body["pinned"]), elements_are![eq("In progress")]);
    expect_that!(titles(&response.body["ranked"]), elements_are![eq("Open")]);
    expect_that!(titles(&response.body["blocked"]), is_empty());
    expect_that!(response.body["hidden_count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn each_state_can_be_pinned_shown_or_hidden() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    one_of_each_state(&app, &session, &heap).await?;

    // Act
    let response = list(&app, &session, &heap, "open=hidden&in_progress=shown&blocked=pinned").await?;

    // Assert
    expect_that!(titles(&response.body["pinned"]), elements_are![eq("Blocked")]);
    expect_that!(titles(&response.body["ranked"]), elements_are![eq("In progress")]);
    expect_that!(response.body["hidden_count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn shown_blocked_items_are_grouped_or_mixed_in_by_priority() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    one_of_each_state(&app, &session, &heap).await?;

    // Act
    let grouped = list(&app, &session, &heap, "blocked=shown").await?;
    let mixed = list(&app, &session, &heap, "blocked=shown&blocked_layout=mixed").await?;

    // Assert
    expect_that!(titles(&grouped.body["ranked"]), elements_are![eq("Open")]);
    expect_that!(titles(&grouped.body["blocked"]), elements_are![eq("Blocked")]);
    expect_that!(titles(&mixed.body["ranked"]), elements_are![eq("Blocked"), eq("Open")]);
    expect_that!(titles(&mixed.body["blocked"]), is_empty());
    expect_that!(mixed.body["hidden_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_tag_filter_keeps_matching_items_in_priority_order() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let infra = app.new_tag_id(&session, &heap, "infra").await?;
    let docs = app.new_tag_id(&session, &heap, "docs").await?;
    let other = app.new_tag_id(&session, &heap, "other").await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Large", "effort": "l", "tag_ids": [infra] }),
    )
    .await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Medium", "effort": "m", "tag_ids": [other] }),
    )
    .await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Small", "effort": "s", "tag_ids": [docs, infra] }),
    )
    .await?;

    // Act
    let response = list(&app, &session, &heap, &format!("tag={infra},{docs}")).await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::OK));
    expect_that!(
        titles(&response.body["ranked"]),
        elements_are![eq("Small"), eq("Large")]
    );
    expect_that!(response.body["ranked"][1]["position"], eq(&json!(2)));
    expect_that!(response.body["hidden_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blocker_keeps_the_priority_of_a_blocked_item_that_is_filtered_out() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let infra = app.new_tag_id(&session, &heap, "infra").await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Medium", "effort": "m", "tag_ids": [infra] }),
    )
    .await?;
    let blocker = add(
        &app,
        &session,
        &heap,
        json!({ "title": "Large blocker", "effort": "l", "tag_ids": [infra] }),
    )
    .await?;
    let urgent = add(&app, &session, &heap, json!({ "title": "Urgent", "effort": "s" })).await?;
    let blocked = app
        .patch(
            &session,
            &app.item_url(&urgent).await?,
            &json!(1),
            json!({ "state": "blocked", "blocked_by_item_id": blocker }),
        )
        .await?;
    anyhow::ensure!(blocked.status == StatusCode::OK, "block failed: {}", blocked.body);

    // Act
    let response = list(&app, &session, &heap, &format!("tag={infra}")).await?;

    // Assert
    expect_that!(
        titles(&response.body["ranked"]),
        elements_are![eq("Large blocker"), eq("Medium")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_size_filters_keep_items_with_one_of_the_sizes() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Small", "effort": "s", "impact": "xl" }),
    )
    .await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Medium", "effort": "m", "impact": "s" }),
    )
    .await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Large", "effort": "l", "impact": "xl" }),
    )
    .await?;
    add(&app, &session, &heap, json!({ "title": "Unsized" })).await?;

    // Act
    let response = list(&app, &session, &heap, "effort=m,l&impact=xl").await?;

    // Assert
    expect_that!(titles(&response.body["ranked"]), elements_are![eq("Large")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sorting_by_due_lists_the_soonest_first_and_renumbers_positions() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let due = |date: &str| json!({ "date": date, "firm": false });
    add(&app, &session, &heap, json!({ "title": "Small", "effort": "s" })).await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Medium", "effort": "m", "due": due("2027-06-01") }),
    )
    .await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Large", "effort": "l", "due": due("2027-03-01") }),
    )
    .await?;

    // Act
    let response = list(&app, &session, &heap, "sort=due").await?;

    // Assert
    expect_that!(
        titles(&response.body["ranked"]),
        elements_are![eq("Large"), eq("Medium"), eq("Small")]
    );
    expect_that!(response.body["ranked"][0]["position"], eq(&json!(1)));
    expect_that!(response.body["ranked"][2]["position"], eq(&json!(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_tag_of_another_heap_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    let foreign = app.new_tag_id(&other_session, &other_heap, "infra").await?;

    // Act
    let response = list(&app, &session, &heap, &format!("tag={foreign}")).await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(response.body["code"], eq(&json!("tag_not_in_heap")));
    expect_that!(response.body["field"], eq(&json!("tag")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_sort_is_a_bad_request() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;

    // Act
    let response = list(&app, &session, &heap, "sort=title").await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(response.body["code"], eq(&json!("invalid_query")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn grouped_blocked_items_stay_in_priority_order_whatever_the_sort() -> Result<()> {
    // Arrange: small waits for large, so large inherits its priority and comes first; by effort it would be last.
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let small = add(&app, &session, &heap, json!({ "title": "Small", "effort": "s" })).await?;
    let large = add(&app, &session, &heap, json!({ "title": "Large", "effort": "l" })).await?;
    move_to(&app, &session, &large, "blocked").await?;
    let waits = app
        .patch(
            &session,
            &app.item_url(&small).await?,
            &json!(1),
            json!({ "state": "blocked", "blocked_by_item_id": large }),
        )
        .await?;
    anyhow::ensure!(waits.status == StatusCode::OK, "block failed: {}", waits.body);

    // Act
    let by_priority = list(&app, &session, &heap, "blocked=shown").await?;
    let by_effort = list(&app, &session, &heap, "blocked=shown&sort=effort").await?;

    // Assert
    expect_that!(
        titles(&by_effort.body["blocked"]),
        eq(&titles(&by_priority.body["blocked"]))
    );
    expect_that!(
        titles(&by_priority.body["blocked"]),
        elements_are![eq("Large"), eq("Small")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sorting_by_effort_and_by_impact_puts_unset_values_last() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    add(&app, &session, &heap, json!({ "title": "Unsized" })).await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Large", "effort": "l", "impact": "xl" }),
    )
    .await?;
    add(
        &app,
        &session,
        &heap,
        json!({ "title": "Small", "effort": "s", "impact": "s" }),
    )
    .await?;

    // Act
    let by_effort = list(&app, &session, &heap, "sort=effort").await?;
    let by_impact = list(&app, &session, &heap, "sort=impact").await?;

    // Assert
    expect_that!(
        titles(&by_effort.body["ranked"]),
        elements_are![eq("Small"), eq("Large"), eq("Unsized")]
    );
    expect_that!(
        titles(&by_impact.body["ranked"]),
        elements_are![eq("Large"), eq("Small"), eq("Unsized")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_hidden_count_only_counts_items_the_filters_keep() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let infra = app.new_tag_id(&session, &heap, "infra").await?;
    let tagged = add(&app, &session, &heap, json!({ "title": "Tagged", "tag_ids": [infra] })).await?;
    let untagged = add(&app, &session, &heap, json!({ "title": "Untagged" })).await?;
    for item in [&tagged, &untagged] {
        move_to(&app, &session, item, "blocked").await?;
    }

    // Act
    let filtered = list(&app, &session, &heap, &format!("tag={infra}")).await?;
    let unfiltered = list(&app, &session, &heap, "tag=").await?;

    // Assert
    expect_that!(filtered.body["hidden_count"], eq(&json!(1)));
    expect_that!(unfiltered.status, eq(StatusCode::OK));
    expect_that!(unfiltered.body["hidden_count"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn malformed_list_values_are_bad_requests() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;

    for query in ["effort=xs", "impact=s,huge", "tag=not-a-uuid", "blocked=collapsed"] {
        // Act
        let response = list(&app, &session, &heap, query).await?;

        // Assert
        expect_that!(response.status, eq(StatusCode::BAD_REQUEST), "{query}");
        expect_that!(response.body["code"], eq(&json!("invalid_query")), "{query}");
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_non_member_filtering_by_a_tag_of_the_heap_gets_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (owner, heap) = app.register("ada@example.test").await?;
    let infra = app.new_tag_id(&owner, &heap, "infra").await?;
    let (stranger, _) = app.register("eve@example.test").await?;

    // Act
    let response = list(&app, &stranger, &heap, &format!("tag={infra}")).await?;

    // Assert
    expect_that!(response.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}
