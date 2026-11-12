//! Reminders: a third item kind with a date. Before the date it is out of the ranked list, the counts and the agents'
//! queue; from the date on it is an ordinary open item. The test clock starts at 12:00 UTC on 1 Jan 2027, and the
//! harness sends Europe/Berlin, so a reminder for 5 Jan joins the list at 23:00 UTC on 4 Jan.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{START, TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

/// 23:00 UTC on 4 Jan 2027: the start of 5 Jan in Berlin.
const JOINS_ON_THE_5TH: i64 = START + 3 * 86_400_000 + 11 * 3_600_000;

struct World {
    app: TestApp,
    session: String,
    heap: String,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    Ok(World { app, session, heap })
}

impl World {
    /// Adds a reminder for `date` and returns the response.
    async fn remind(&self, title: &str, date: &str) -> Result<TestResponse> {
        self.app
            .create_item(
                &self.session,
                &self.heap,
                json!({ "title": title, "kind": "reminder", "due": { "date": date, "firm": false } }),
            )
            .await
    }

    async fn item_path(&self, key: &str) -> Result<String> {
        Ok(format!("{}/items/{key}", self.app.heap_url(&self.heap).await?))
    }

    async fn get(&self, key: &str) -> Result<Value> {
        let fetched = self
            .app
            .send(Method::GET, &self.item_path(key).await?, Some(&self.session), None)
            .await?;
        Ok(fetched.body)
    }

    async fn edit(&self, key: &str, body: Value) -> Result<TestResponse> {
        let version = self.get(key).await?["version"].clone();
        self.app
            .patch(&self.session, &self.item_path(key).await?, &version, body)
            .await
    }

    /// The heap's list with `query` (such as `reminders=shown`).
    async fn list(&self, query: &str) -> Result<Value> {
        let list = self
            .app
            .send(
                Method::GET,
                &format!("{}/items?{query}", self.app.heap_url(&self.heap).await?),
                Some(&self.session),
                None,
            )
            .await?;
        anyhow::ensure!(list.status == StatusCode::OK, "list failed: {}", list.body);
        Ok(list.body)
    }

    /// The row of the item with `key` among the ranked groups.
    async fn ranked_row(&self, key: &str) -> Result<Option<Value>> {
        let list = self.list("").await?;
        Ok(["pinned", "ranked", "blocked"]
            .iter()
            .filter_map(|group| list[group].as_array())
            .flatten()
            .find(|row| row["item"]["key"] == json!(key))
            .cloned())
    }

    /// The heap's `open_count` as `/me` and the heap itself report it.
    async fn open_counts(&self) -> Result<[Value; 2]> {
        let me = self
            .app
            .send(Method::GET, "/api/v1/me", Some(&self.session), None)
            .await?;
        let in_me = me.body["teams"][0]["heaps"][0]["open_count"].clone();
        let own = self
            .app
            .send(
                Method::GET,
                &self.app.heap_url(&self.heap).await?,
                Some(&self.session),
                None,
            )
            .await?;
        Ok([in_me, own.body["open_count"].clone()])
    }
}

fn keys(items: &Value) -> Vec<&str> {
    items
        .as_array()
        .map(|items| items.iter().filter_map(|item| item["key"].as_str()).collect())
        .unwrap_or_default()
}

fn code(response: &TestResponse) -> Option<&str> {
    response.body["code"].as_str()
}

#[gtest]
#[tokio::test]
async fn a_reminder_has_a_firm_due_and_arrives_at_the_start_of_its_date() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act: a loose due is accepted for its date, and made firm.
    let created = w.remind("Renew the domain", "2027-01-05").await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(created.body["kind"], eq(&json!("reminder")));
    expect_that!(created.body["stage"], eq(&json!("as_written")));
    expect_that!(created.body["due"]["date"], eq(&json!("2027-01-05")));
    expect_that!(created.body["due"]["firm"], eq(&json!(true)));
    expect_that!(created.body["arrives_at"], eq(&json!(JOINS_ON_THE_5TH)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_without_a_date_is_refused() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let refused = w
        .app
        .create_item(&w.session, &w.heap, json!({ "title": "Renew", "kind": "reminder" }))
        .await?;
    let list = w.list("reminders=shown").await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&refused), some(eq("reminder_needs_date")));
    expect_that!(refused.body["field"], eq(&json!("due")));
    expect_that!(list["reminders"], eq(&json!([])));
    expect_that!(list["ranked"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_to_come_is_out_of_the_ranking_and_every_count() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.app
        .new_item(&w.session, &w.heap, json!({ "title": "A note" }))
        .await?;
    let before = w.open_counts().await?;

    // Act
    let reminder = w.remind("Renew the domain", "2027-01-05").await?;
    let key = reminder.body["key"].as_str().context("key")?;
    let after = w.open_counts().await?;
    let list = w.list("").await?;

    // Assert
    expect_that!(after, eq(&before));
    expect_that!(after, eq(&[json!(1), json!(1)]));
    expect_that!(w.ranked_row(key).await?, none());
    expect_that!(list["unfiltered_count"], eq(&json!(1)));
    expect_that!(list["hidden_count"], eq(&json!(0)));
    expect_that!(list["reminders"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_list_shows_reminders_to_come_soonest_first_when_asked() -> Result<()> {
    // Arrange: made out of order.
    let w = world().await?;
    w.remind("Later", "2027-02-01").await?;
    w.remind("Sooner", "2027-01-05").await?;
    let ops = w.app.new_tag_id(&w.session, &w.heap, "ops").await?;
    let tagged = w
        .app
        .new_item(
            &w.session,
            &w.heap,
            json!({ "title": "Tagged", "kind": "reminder", "due": { "date": "2027-03-01", "firm": true }, "tag_ids": [ops] }),
        )
        .await?;

    // Act
    let shown = w.list("reminders=shown").await?;
    let filtered = w.list(&format!("reminders=shown&tag={ops}")).await?;
    let hidden = w.list("reminders=hidden").await?;

    // Assert
    let titles: Vec<&str> = shown["reminders"]
        .as_array()
        .context("reminders")?
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect();
    expect_that!(titles, elements_are![eq(&"Sooner"), eq(&"Later"), eq(&"Tagged")]);
    expect_that!(
        keys(&filtered["reminders"]),
        elements_are![eq(&tagged["key"].as_str().context("key")?)]
    );
    expect_that!(hidden["reminders"], eq(&json!([])));
    expect_that!(shown["ranked"], eq(&json!([])));
    expect_that!(shown["unfiltered_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_is_ranked_as_due_that_day_once_its_date_comes() -> Result<()> {
    // Arrange
    let w = world().await?;
    let reminder = w.remind("Renew the domain", "2027-01-05").await?;
    let key = reminder.body["key"].as_str().context("key")?;

    // Act: the day arrives.
    w.app.clock.advance(Duration::from_hours(3 * 24 + 12));
    let row = w.ranked_row(key).await?.context("listed on its date")?;
    let list = w.list("reminders=shown").await?;
    let counts = w.open_counts().await?;

    // Assert
    let labels: Vec<&str> = row["contributors"]
        .as_array()
        .context("contributors")?
        .iter()
        .filter_map(|c| c["label"].as_str())
        .collect();
    expect_that!(labels, contains(eq(&"Due within a day")));
    expect_that!(row["item"]["kind"], eq(&json!("reminder")));
    expect_that!(list["reminders"], eq(&json!([])));
    expect_that!(counts, eq(&[json!(1), json!(1)]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_date_today_or_in_the_past_is_listed_at_once() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let today = w.remind("Today", "2027-01-01").await?;
    let past = w.remind("Past", "2026-12-20").await?;

    // Assert
    for created in [&today, &past] {
        expect_that!(created.status, eq(StatusCode::CREATED));
        let key = created.body["key"].as_str().context("key")?;
        expect_that!(w.ranked_row(key).await?, some(anything()));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_becomes_a_task_in_place_and_is_listed_at_once() -> Result<()> {
    // Arrange
    let w = world().await?;
    let reminder = w.remind("Renew the domain", "2027-01-05").await?;
    let key = reminder.body["key"].as_str().context("key")?;

    // Act
    let made = w.edit(key, json!({ "kind": "task" })).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(made.body["key"], eq(&reminder.body["key"]));
    expect_that!(made.body["kind"], eq(&json!("task")));
    expect_that!(made.body["stage"], eq(&json!("as_written")));
    expect_that!(made.body["arrives_at"], eq(&Value::Null));
    expect_that!(made.body["due"]["date"], eq(&json!("2027-01-05")));
    expect_that!(made.body["due"]["firm"], eq(&json!(true)));
    expect_that!(w.ranked_row(key).await?, some(anything()));

    // It ages from the change, not from the date it was set for, which is still ahead.
    w.app.clock.advance(Duration::from_days(21));
    let row = w.ranked_row(key).await?.context("listed")?;
    let labels: Vec<&str> = row["contributors"]
        .as_array()
        .context("contributors")?
        .iter()
        .filter_map(|c| c["label"].as_str())
        .collect();
    expect_that!(labels, contains(starts_with("Waiting 3 weeks")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_list_frozen_before_a_reminders_date_ranks_it_once_the_date_has_come() -> Result<()> {
    // Arrange: a list read before the date, then the date comes.
    let w = world().await?;
    let reminder = w.remind("Renew the domain", "2027-01-02").await?;
    let key = reminder.body["key"].as_str().context("key")?;
    let frozen = w.list("").await?["as_of"].as_i64().context("as_of")?;
    w.app.clock.advance(Duration::from_hours(12));

    // Act
    let list = w.list(&format!("as_of={frozen}&reminders=shown")).await?;

    // Assert: ranked as of the frozen time, but no longer to come.
    expect_that!(list["as_of"], eq(&json!(frozen)));
    expect_that!(list["reminders"], eq(&json!([])));
    let ranked = list["ranked"].as_array().context("ranked")?;
    expect_that!(
        ranked.iter().filter(|row| row["item"]["key"] == json!(key)).count(),
        eq(1)
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminders_date_moves_and_cannot_be_cleared() -> Result<()> {
    // Arrange: a reminder that has already arrived.
    let w = world().await?;
    let reminder = w.remind("Renew the domain", "2027-01-03").await?;
    let key = reminder.body["key"].as_str().context("key")?;
    w.app.clock.advance(Duration::from_days(2));
    expect_that!(w.ranked_row(key).await?, some(anything()));

    // Act
    let cleared = w.edit(key, json!({ "due": null })).await?;
    let moved = w
        .edit(key, json!({ "due": { "date": "2027-01-05", "firm": false } }))
        .await?;

    // Assert: it leaves the list until its new date.
    expect_that!(cleared.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&cleared), some(eq("reminder_needs_date")));
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["due"]["firm"], eq(&json!(true)));
    expect_that!(moved.body["arrives_at"], eq(&json!(JOINS_ON_THE_5TH)));
    expect_that!(w.ranked_row(key).await?, none());
    expect_that!(
        keys(&w.list("reminders=shown").await?["reminders"]),
        elements_are![eq(&key)]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn only_a_note_or_a_reminder_can_become_a_task() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "A note" }))
        .await?;
    let task = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "A task", "kind": "task" }))
        .await?;
    let reminder = w.remind("A reminder", "2027-01-05").await?;
    let key = |item: &Value| item["key"].as_str().map(str::to_owned).context("key");

    // Act
    let note_to_reminder = w.edit(&key(&note)?, json!({ "kind": "reminder" })).await?;
    let task_to_reminder = w.edit(&key(&task)?, json!({ "kind": "reminder" })).await?;
    let reminder_to_note = w.edit(&key(&reminder.body)?, json!({ "kind": "note" })).await?;

    // Assert
    for refused in [&note_to_reminder, &task_to_reminder, &reminder_to_note] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(code(refused), some(eq("kind_change_not_allowed")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_cant_be_sent_to_an_agent_until_it_is_a_task() -> Result<()> {
    // Arrange
    let w = world().await?;
    let reminder = w.remind("Renew the domain", "2027-01-01").await?;
    let key = reminder.body["key"].as_str().context("key")?;
    let turn_path = format!("{}/turn", w.item_path(key).await?);
    let actions: Vec<&str> = reminder.body["allowed_actions"]
        .as_array()
        .context("actions")?
        .iter()
        .filter_map(Value::as_str)
        .collect();

    // Act
    let refused = w
        .app
        .send(
            Method::POST,
            &turn_path,
            Some(&w.session),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    w.edit(key, json!({ "kind": "task" })).await?;
    let sent = w
        .app
        .send(
            Method::POST,
            &turn_path,
            Some(&w.session),
            Some(json!({ "reach": "one" })),
        )
        .await?;

    // Assert
    expect_that!(actions, contains(eq(&"make_task")));
    expect_that!(actions, not(contains(eq(&"send_to_agent"))));
    expect_that!(actions, not(contains(eq(&"add_step"))));
    expect_that!(reminder.body["next_goal"], eq(&Value::Null));
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&refused), some(eq("turn_needs_note_or_task")));
    expect_that!(sent.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_that_prepares_gives_a_new_reminder_no_turn_and_the_queue_skips_it() -> Result<()> {
    // Arrange
    let w = world().await?;
    let path = w.app.heap_url(&w.heap).await?;
    let heap = w.app.send(Method::GET, &path, Some(&w.session), None).await?;
    let on = w
        .app
        .patch(
            &w.session,
            &path,
            &heap.body["version"],
            json!({ "agents_prepare": true }),
        )
        .await?;
    expect_that!(on.status, eq(StatusCode::OK));

    // Act
    let reminder = w.remind("Renew the domain", "2027-01-05").await?;
    let note = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "A note" }))
        .await?;
    let heap_ref = path
        .strip_prefix("/api/v1/teams/")
        .context("heap url")?
        .replace("/heaps/", "/");
    let queue = w
        .app
        .send(
            Method::GET,
            &format!("/api/v1/work/next?heaps={heap_ref}"),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(reminder.body["turn"], eq(&Value::Null));
    expect_that!(note["turn"], not(eq(&Value::Null)));
    expect_that!(queue.status, eq(StatusCode::OK));
    expect_that!(queue.body["item"]["key"], not(eq(&reminder.body["key"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_saved_view_keeps_the_reminders_setting() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let saved = w
        .app
        .send(
            Method::POST,
            &format!("{}/views", w.app.heap_url(&w.heap).await?),
            Some(&w.session),
            Some(json!({ "name": "With reminders", "view": { "reminders": "shown" } })),
        )
        .await?;
    let plain = w
        .app
        .send(
            Method::POST,
            &format!("{}/views", w.app.heap_url(&w.heap).await?),
            Some(&w.session),
            Some(json!({ "name": "Plain", "view": {} })),
        )
        .await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::CREATED));
    expect_that!(saved.body["view"]["reminders"], eq(&json!("shown")));
    expect_that!(plain.body["view"]["reminders"], eq(&json!("hidden")));
    Ok(())
}
