//! Placing an item by hand in its heap's list: where it lands, what stays the same, the reset, the history and the
//! refusals.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

fn id(body: &Value) -> String {
    body["id"].as_str().unwrap_or_default().to_owned()
}

/// A user with four open items that rank in this order by their need: `a`, `b`, `c`, `d`.
struct World {
    app: TestApp,
    session: String,
    heap: String,
    items: Vec<Value>,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let mut items = Vec::new();
    for (title, need) in [
        ("a", "critical"),
        ("b", "expected"),
        ("c", "nice_to_have"),
        ("d", "idea"),
    ] {
        items.push(
            app.new_item(&session, &heap, json!({ "title": title, "need": need }))
                .await?,
        );
    }
    Ok(World {
        app,
        session,
        heap,
        items,
    })
}

impl World {
    async fn url(&self, item: usize) -> Result<String> {
        self.app.item_url(id(&self.items[item])).await
    }

    async fn read(&self, item: usize) -> Result<Value> {
        Ok(self
            .app
            .send(Method::GET, &self.url(item).await?, Some(&self.session), None)
            .await?
            .body)
    }

    /// Places item `item` on `side` of `anchor`, at the version it has now.
    async fn place(&self, item: usize, side: &str, anchor: usize) -> Result<TestResponse> {
        let version = self.read(item).await?["version"].clone();
        self.place_at(item, &version, side, &id(&self.items[anchor])).await
    }

    async fn place_at(&self, item: usize, version: &Value, side: &str, anchor: &str) -> Result<TestResponse> {
        self.app
            .send_with(
                Method::POST,
                &format!("{}/place", self.url(item).await?),
                Some(&self.session),
                Some(json!({ "anchor": anchor, "side": side })),
                &[CSRF, ("if-match", &version.to_string())],
            )
            .await
    }

    async fn reset(&self, item: usize) -> Result<TestResponse> {
        let version = self.read(item).await?["version"].clone();
        self.app
            .send_with(
                Method::DELETE,
                &format!("{}/place", self.url(item).await?),
                Some(&self.session),
                None,
                &[CSRF, ("if-match", &version.to_string())],
            )
            .await
    }

    /// The titles of the ranked list, with the first contributor of each.
    async fn list(&self) -> Result<Vec<(String, Option<String>)>> {
        let list = self
            .app
            .send(
                Method::GET,
                &format!("{}/items", self.app.heap_url(&self.heap).await?),
                Some(&self.session),
                None,
            )
            .await?;
        Ok(list.body["ranked"]
            .as_array()
            .context("no ranked list")?
            .iter()
            .map(|entry| {
                (
                    entry["item"]["title"].as_str().unwrap_or_default().to_owned(),
                    entry["contributors"][0]["label"].as_str().map(str::to_owned),
                )
            })
            .collect())
    }

    async fn titles(&self) -> Result<Vec<String>> {
        Ok(self.list().await?.into_iter().map(|(title, _)| title).collect())
    }

    async fn history(&self, item: usize) -> Result<Value> {
        Ok(self
            .app
            .send(
                Method::GET,
                &format!(
                    "{}/audit?entity_id={}",
                    self.app.heap_url(&self.heap).await?,
                    id(&self.items[item])
                ),
                Some(&self.session),
                None,
            )
            .await?
            .body)
    }
}

#[gtest]
#[tokio::test]
async fn placing_above_another_item_puts_it_right_above_and_changes_no_field() -> Result<()> {
    // Arrange
    let w = world().await?;
    let before = w.read(3).await?;

    // Act
    let placed = w.place(3, "above", 1).await?;
    let list = w.list().await?;

    // Assert
    expect_that!(placed.status, eq(StatusCode::OK));
    expect_that!(placed.body["placement"], eq(&json!("up")));
    expect_that!(placed.body["version"], eq(&json!(2)));
    for field in ["title", "need", "state", "kind", "effort", "impact", "due"] {
        expect_that!(placed.body[field], eq(&before[field]), "{field}");
    }
    let titles: Vec<&str> = list.iter().map(|(title, _)| title.as_str()).collect();
    expect_that!(titles, elements_are![eq(&"a"), eq(&"d"), eq(&"b"), eq(&"c")]);
    expect_that!(list[1].1, some(eq("Moved up")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn placing_below_another_item_puts_it_right_below() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let placed = w.place(0, "below", 2).await?;
    let list = w.list().await?;

    // Assert
    expect_that!(placed.body["placement"], eq(&json!("down")));
    let titles: Vec<&str> = list.iter().map(|(title, _)| title.as_str()).collect();
    expect_that!(titles, elements_are![eq(&"b"), eq(&"c"), eq(&"a"), eq(&"d")]);
    expect_that!(list[2].1, some(eq("Moved down")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_place_stays_after_a_field_edit() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.place(3, "above", 1).await?;
    let version = w.read(3).await?["version"].clone();

    // Act
    let edited = w
        .app
        .patch(&w.session, &w.url(3).await?, &version, json!({ "title": "d, renamed" }))
        .await?;

    // Assert
    expect_that!(edited.body["placement"], eq(&json!("up")));
    expect_that!(
        w.titles().await?,
        elements_are![eq(&"a"), eq(&"d, renamed"), eq(&"b"), eq(&"c")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn resetting_brings_back_the_order_of_the_fields() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.place(3, "above", 1).await?;

    // Act
    let reset = w.reset(3).await?;

    // Assert
    expect_that!(reset.status, eq(StatusCode::OK));
    expect_that!(reset.body["placement"], eq(&Value::Null));
    expect_that!(w.titles().await?, elements_are![eq(&"a"), eq(&"b"), eq(&"c"), eq(&"d")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn resetting_an_item_that_is_not_placed_writes_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let reset = w.reset(1).await?;
    let history = w.history(1).await?;

    // Assert
    expect_that!(reset.status, eq(StatusCode::OK));
    expect_that!(reset.body["version"], eq(&json!(1)));
    let actions: Vec<&str> = history["entries"]
        .as_array()
        .context("no entries")?
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();
    expect_that!(actions, not(contains(eq(&"item_placed"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_history_says_which_way_the_item_moved_and_that_it_was_reset() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.place(3, "above", 1).await?;
    w.place(3, "below", 2).await?;
    w.reset(3).await?;

    // Act
    let history = w.history(3).await?;

    // Assert
    let entries = history["entries"].as_array().context("no entries")?;
    let phrases: Vec<&str> = entries
        .iter()
        .filter(|entry| entry["action"] == "item_placed")
        .filter_map(|entry| entry["phrase"].as_str())
        .collect();
    expect_that!(
        phrases,
        unordered_elements_are![
            eq(&"moved item up in the list"),
            eq(&"moved item up in the list"),
            eq(&"reset the list place of"),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn moving_to_another_heap_drops_the_place() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.place(3, "above", 1).await?;
    let url = w.app.heap_url(&w.heap).await?;
    let team = url.split("/heaps/").next().context("heap url")?.to_owned();
    let other = w
        .app
        .send(
            Method::POST,
            &format!("{team}/heaps"),
            Some(&w.session),
            Some(json!({ "name": "Other" })),
        )
        .await?;
    let version = w.read(3).await?["version"].clone();

    // Act
    let moved = w
        .app
        .send_with(
            Method::POST,
            &format!("{}/move", w.url(3).await?),
            Some(&w.session),
            Some(json!({ "heap_id": other.body["id"] })),
            &[CSRF, ("if-match", &version.to_string())],
        )
        .await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["placement"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_anchor_in_another_heap_is_refused_like_a_missing_one() -> Result<()> {
    // Arrange
    let w = world().await?;
    let other = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "closed", "need": "expected" }))
        .await?;
    let closed = w
        .app
        .patch(
            &w.session,
            &w.app.item_url(id(&other)).await?,
            &other["version"],
            json!({ "state": "done" }),
        )
        .await?;
    anyhow::ensure!(closed.status == StatusCode::OK, "close failed: {}", closed.body);
    let (_, shared) = w.app.shared_heap(&w.session).await?;
    let elsewhere = w
        .app
        .new_item(&w.session, &shared, json!({ "title": "elsewhere" }))
        .await?;
    let version = w.read(3).await?["version"].clone();

    // Act
    let mut refused = Vec::new();
    for anchor in [
        id(&elsewhere),
        id(&other),
        id(&w.items[3]),
        "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40".to_owned(),
    ] {
        refused.push(w.place_at(3, &version, "above", &anchor).await?);
    }

    // Assert
    for response in &refused {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(response.body["code"], eq(&json!("anchor_not_ranked")));
        expect_that!(response.body["field"], eq(&json!("anchor")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_version_is_a_conflict() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let stale = w.place_at(3, &json!(7), "above", &id(&w.items[1])).await?;

    // Assert
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn someone_outside_the_team_cannot_place_an_item() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (bob, _) = app.register("bob@example.test").await?;
    let (_, heap) = app.shared_heap(&ada).await?;
    let first = app.new_item(&ada, &heap, json!({ "title": "first" })).await?;
    let second = app.new_item(&ada, &heap, json!({ "title": "second" })).await?;

    // Act
    let refused = app
        .send_with(
            Method::POST,
            &format!("{}/place", app.item_url(id(&second)).await?),
            Some(&bob),
            Some(json!({ "anchor": id(&first), "side": "above" })),
            &[CSRF, ("if-match", "1")],
        )
        .await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_may_not_place_an_item() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let refused = w
        .app
        .send_with(
            Method::POST,
            &format!("{}/place", w.url(3).await?),
            Some(&w.session),
            Some(json!({ "anchor": id(&w.items[1]), "side": "above" })),
            &[
                CSRF,
                ("upheap-run", "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40"),
                ("if-match", "1"),
            ],
        )
        .await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(refused.body["code"], eq(&json!("run_forbidden")));
    Ok(())
}
