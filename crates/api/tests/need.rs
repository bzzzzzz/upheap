//! Need: required on every new item, changed but never cleared, and counted in the ranking.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::json;

#[gtest]
#[tokio::test]
async fn a_new_item_must_say_how_much_it_is_needed() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let items = format!("{}/items", app.heap_url(&heap).await?);

    // Act
    let missing = app
        .send(Method::POST, &items, Some(&session), Some(json!({ "title": "Task" })))
        .await?;
    let unknown = app
        .send(
            Method::POST,
            &items,
            Some(&session),
            Some(json!({ "title": "Task", "need": "urgent" })),
        )
        .await?;
    let created = app
        .send(
            Method::POST,
            &items,
            Some(&session),
            Some(json!({ "title": "Task", "need": "nice_to_have" })),
        )
        .await?;

    // Assert
    expect_that!(missing.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(missing.body["code"], eq(&json!("invalid_body")));
    expect_that!(unknown.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(unknown.body["code"], eq(&json!("invalid_body")));
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(created.body["need"], eq(&json!("nice_to_have")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn need_can_be_changed_but_not_cleared() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let item = app
        .new_item(&session, &heap, json!({ "title": "Task", "need": "idea" }))
        .await?;
    let path = app.item_url(item["id"].as_str().unwrap_or_default()).await?;

    // Act
    let null = app
        .patch(&session, &path, &item["version"], json!({ "need": null }))
        .await?;
    let changed = app
        .patch(&session, &path, &item["version"], json!({ "need": "critical" }))
        .await?;

    // Assert
    expect_that!(null.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(null.body["code"], eq(&json!("invalid_body")));
    expect_that!(changed.status, eq(StatusCode::OK));
    expect_that!(changed.body["need"], eq(&json!("critical")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_critical_item_ranks_above_an_idea_and_says_why() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Dark mode", "need": "idea", "effort": "s", "impact": "xl" }),
    )
    .await?;
    app.clock.advance(domain::Duration::from_days(10));
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Renew the certificate", "need": "critical" }),
    )
    .await?;

    // Act
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(
        list.body["ranked"][0]["item"]["title"],
        eq(&json!("Renew the certificate"))
    );
    expect_that!(
        list.body["ranked"][0]["contributors"][0],
        eq(&json!({ "factor": "need", "label": "Need: Critical", "share_percent": 100 }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_ranked_item_says_what_holds_it_back_and_the_first_one_says_nothing() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Renew the certificate", "need": "critical", "effort": "s", "impact": "xl" }),
    )
    .await?;
    app.new_item(&session, &heap, json!({ "title": "Dark mode", "need": "idea" }))
        .await?;

    // Act
    let list = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(list.body["ranked"][0]["held_back_by"], eq(&json!([])));
    expect_that!(
        list.body["ranked"][1]["held_back_by"],
        eq(&json!([
            { "gap": "need", "label": "Need: Idea" },
            { "gap": "due", "label": "No due date" },
            { "gap": "size", "label": "Effort and impact not set" },
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_first_item_listed_says_nothing_even_when_it_is_not_the_top_of_the_heap() -> Result<()> {
    // Arrange: a filter leaves out the heap's top item, so the two left are first and second in the list.
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Renew the certificate", "need": "critical", "effort": "s", "impact": "xl" }),
    )
    .await?;
    for title in ["Dark mode", "Export"] {
        app.new_item(
            &session,
            &heap,
            json!({ "title": title, "need": "idea", "effort": "m" }),
        )
        .await?;
    }

    // Act
    let list = app
        .send(
            Method::GET,
            &format!("{}/items?effort=m", app.heap_url(&heap).await?),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(list.body["ranked"][0]["position"], eq(&json!(1)));
    expect_that!(list.body["ranked"][0]["held_back_by"], eq(&json!([])));
    expect_that!(
        list.body["ranked"][1]["held_back_by"],
        eq(&json!([
            { "gap": "need", "label": "Need: Idea" },
            { "gap": "due", "label": "No due date" },
            { "gap": "size", "label": "Impact not set, counts as S" },
        ]))
    );
    Ok(())
}
