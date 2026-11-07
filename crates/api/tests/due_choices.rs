//! The rough due times a due picker offers, and what an item due at one of them reads back.

mod common;

use anyhow::Result;
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};

#[gtest]
#[tokio::test]
async fn an_item_due_at_a_choice_reads_back_with_its_label() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;

    // Act
    let response = app
        .send(Method::GET, "/api/v1/due-choices", Some(&session), None)
        .await?;
    let choices = response.body["choices"].as_array().cloned().unwrap_or_default();
    let mut read_back = Vec::new();
    for choice in &choices {
        let due = json!({ "date": choice["date"], "firm": false });
        let item = app
            .new_item(&session, &heap, json!({ "title": "Call the plumber", "due": due }))
            .await?;
        read_back.push((choice["label"].clone(), item["due"]["label"].clone()));
    }

    // Assert
    expect_that!(response.status, eq(StatusCode::OK));
    let labels: Vec<&Value> = choices.iter().map(|choice| &choice["label"]).collect();
    expect_that!(
        labels,
        elements_are![
            eq(&&json!("in a few days")),
            eq(&&json!("next week")),
            eq(&&json!("in a few weeks")),
            eq(&&json!("next month")),
            eq(&&json!("in a few months"))
        ]
    );
    for (offered, shown) in read_back {
        expect_that!(shown, eq(&offered));
    }
    Ok(())
}
