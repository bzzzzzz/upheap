//! The Agents page: a run records what it reached, the history of runs that ended as reached, and the decisions that
//! wait for the person.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};
use service::Clock;

/// A user with a personal heap.
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

fn key_of(item: &Value) -> &str {
    item["key"].as_str().unwrap_or_default()
}

fn titles(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .map(|rows| rows.iter().filter_map(|row| row["item"]["title"].as_str()).collect())
        .unwrap_or_default()
}

impl World {
    async fn item_path_in(&self, heap: &str, key: &str) -> Result<String> {
        Ok(format!("{}/items/{key}", self.app.heap_url(heap).await?))
    }

    async fn item_path(&self, key: &str) -> Result<String> {
        self.item_path_in(&self.heap, key).await
    }

    async fn heap_ref(&self, heap: &str) -> Result<String> {
        let url = self.app.heap_url(heap).await?;
        Ok(url
            .strip_prefix("/api/v1/teams/")
            .context("heap url")?
            .replace("/heaps/", "/"))
    }

    async fn user_id(&self) -> Result<String> {
        let me = self
            .app
            .send(Method::GET, "/api/v1/me", Some(&self.session), None)
            .await?;
        Ok(me.body["user"]["id"].as_str().context("user id")?.to_owned())
    }

    /// Adds an item with a context in `heap` and returns it.
    async fn add_in(&self, heap: &str, title: &str, need: &str) -> Result<Value> {
        self.app
            .new_item(
                &self.session,
                heap,
                json!({ "title": title, "need": need, "description": "Everything is a mess." }),
            )
            .await
    }

    async fn add(&self, title: &str) -> Result<Value> {
        self.add_in(&self.heap, title, "expected").await
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

    async fn send_turn(&self, heap: &str, key: &str) -> Result<()> {
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{}/turn", self.item_path_in(heap, key).await?),
                Some(&self.session),
                Some(json!({ "reach": "one" })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        Ok(())
    }

    /// Claims the item sent to an agent in `heap` and returns the run's ID.
    async fn claim_in(&self, heap: &str) -> Result<String> {
        let claimed = self
            .app
            .send(
                Method::POST,
                "/api/v1/work/runs",
                Some(&self.session),
                Some(json!({ "heaps": [self.heap_ref(heap).await?] })),
            )
            .await?;
        anyhow::ensure!(claimed.status == StatusCode::CREATED, "claim failed: {}", claimed.body);
        Ok(claimed.body["run"]["id"].as_str().context("no run")?.to_owned())
    }

    async fn run_call(&self, run: &str, action: &str, body: Value) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("/api/v1/work/runs/{run}/{action}"),
                Some(&self.session),
                Some(body),
            )
            .await
    }

    async fn reached_with(&self, run: &str, body: Value) -> Result<()> {
        let reached = self.run_call(run, "reached", body).await?;
        anyhow::ensure!(reached.status == StatusCode::OK, "reached failed: {}", reached.body);
        Ok(())
    }

    /// Sends the note in `heap` to an agent, and has the agent reach its goal. Returns the run's ID.
    async fn reach_in(&self, heap: &str, key: &str) -> Result<String> {
        self.send_turn(heap, key).await?;
        let run = self.claim_in(heap).await?;
        self.reached_with(&run, json!({ "result": "Added the context" }))
            .await?;
        Ok(run)
    }

    /// A note with a context that an agent reached in the personal heap. Returns its key and the run.
    async fn reached_note(&self, title: &str) -> Result<(String, String)> {
        let note = self.add(title).await?;
        let run = self.reach_in(&self.heap, key_of(&note)).await?;
        Ok((key_of(&note).to_owned(), run))
    }

    /// A request in `run`, as the CLI makes it.
    async fn in_run(&self, run: &str, method: Method, path: &str, body: Option<Value>) -> Result<TestResponse> {
        self.app
            .send_with(method, path, Some(&self.session), body, &[CSRF, ("upheap-run", run)])
            .await
    }

    async fn decisions(&self) -> Result<Value> {
        let decisions = self
            .app
            .send(Method::GET, "/api/v1/me/decisions", Some(&self.session), None)
            .await?;
        anyhow::ensure!(
            decisions.status == StatusCode::OK,
            "decisions failed: {}",
            decisions.body
        );
        Ok(decisions.body)
    }

    async fn results(&self, query: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("/api/v1/me/results{query}"),
                Some(&self.session),
                None,
            )
            .await
    }

    /// A note with a context, split in a run into one task with a step, then became. Returns the note's key and the
    /// task.
    async fn became_note(&self, title: &str) -> Result<(String, Value)> {
        let note = self.add(title).await?;
        let key = key_of(&note).to_owned();
        self.edit(&key, json!({ "stage": "with_context" })).await?;
        self.send_turn(&self.heap, &key).await?;
        let run = self.claim_in(&self.heap).await?;
        let items = format!("{}/items", self.app.heap_url(&self.heap).await?);
        let task = self
            .in_run(
                &run,
                Method::POST,
                &items,
                Some(json!({
                    "title": format!("{title}, the task"),
                    "need": "expected",
                    "kind": "task",
                    "description": "Done when: it works.",
                    "origin": { "item_id": note["id"], "age": "keep" },
                })),
            )
            .await?;
        anyhow::ensure!(task.status == StatusCode::CREATED, "split failed: {}", task.body);
        let step = self
            .in_run(
                &run,
                Method::POST,
                &format!("{items}/{}/steps", key_of(&task.body)),
                Some(json!({ "title": "Do it" })),
            )
            .await?;
        anyhow::ensure!(step.status == StatusCode::CREATED, "step failed: {}", step.body);
        let became = self
            .in_run(&run, Method::POST, &format!("{items}/{key}/became"), None)
            .await?;
        anyhow::ensure!(became.status == StatusCode::OK, "became failed: {}", became.body);
        Ok((key, task.body))
    }
}

#[gtest]
#[tokio::test]
async fn a_run_records_what_it_reached_and_the_other_ends_record_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act: a note reached with context; a note and a task whose turn to with context is reported as defined; a note
    // that became; a run that failed; a run that handed the turn back.
    let (_, with_context) = w.reached_note("Rotate the certs").await?;

    let past = w.add("Rotate the keys").await?;
    w.send_turn(&w.heap, key_of(&past)).await?;
    let past_run = w.claim_in(&w.heap).await?;
    let path = w.item_path(key_of(&past)).await?;
    let version = w.get(key_of(&past)).await?["version"].clone();
    let made_task = w
        .app
        .send_with(
            Method::PATCH,
            &path,
            Some(&w.session),
            Some(json!({ "kind": "task" })),
            &[CSRF, ("upheap-run", &past_run), ("if-match", &version.to_string())],
        )
        .await?;
    let step = w
        .in_run(
            &past_run,
            Method::POST,
            &format!("{path}/steps"),
            Some(json!({ "title": "Order" })),
        )
        .await?;
    w.reached_with(&past_run, json!({ "result": "Went straight on", "stage": "defined" }))
        .await?;

    w.became_note("Rotate the locks").await?;

    let failing = w.add("Rotate the seals").await?;
    w.send_turn(&w.heap, key_of(&failing)).await?;
    let failed = w.claim_in(&w.heap).await?;
    w.run_call(&failed, "finish", json!({ "failure": "The tool crashed" }))
        .await?;
    let handing = w.add("Rotate the bolts").await?;
    w.send_turn(&w.heap, key_of(&handing)).await?;
    let handed = w.claim_in(&w.heap).await?;
    let handed_back = w
        .run_call(&handed, "hand-back", json!({ "reason": "Nothing to do" }))
        .await?;

    // Assert
    expect_that!(made_task.status, eq(StatusCode::OK));
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(w.app.run_reached(&with_context).await?, some(eq("with_context")));
    expect_that!(w.app.run_reached(&past_run).await?, some(eq("defined")));
    let results = w.results("").await?;
    let reached: Vec<&str> = results.body["rows"]
        .as_array()
        .context("rows")?
        .iter()
        .filter_map(|row| row["reached"].as_str())
        .collect();
    expect_that!(
        reached,
        unordered_elements_are![eq(&"with_context"), eq(&"defined"), eq(&"became")]
    );
    expect_that!(w.app.run_reached(&failed).await?, none());
    expect_that!(handed_back.status, eq(StatusCode::OK));
    expect_that!(w.app.run_reached(&handed).await?, none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn results_hold_reached_runs_newest_first_with_what_they_reached() -> Result<()> {
    // Arrange: a reached run, a failed run, and a later reached run with links.
    let w = world().await?;
    let (first, first_run) = w.reached_note("First note").await?;
    w.app.clock.advance(Duration::from_minutes(5));
    let failing = w.add("Failing note").await?;
    w.send_turn(&w.heap, key_of(&failing)).await?;
    let failed = w.claim_in(&w.heap).await?;
    w.run_call(&failed, "finish", json!({ "failure": "Crashed" })).await?;
    w.app.clock.advance(Duration::from_minutes(5));
    let second = w.add("Second note").await?;
    w.send_turn(&w.heap, key_of(&second)).await?;
    // The failed note is claimed first: take it back so the queue offers the second.
    w.app
        .send(
            Method::DELETE,
            &format!("{}/turn", w.item_path(key_of(&failing)).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let second_run = w.claim_in(&w.heap).await?;
    w.reached_with(
        &second_run,
        json!({ "result": "Opened the pull request", "links": ["https://github.com/acme/app/pull/7"] }),
    )
    .await?;

    // Act
    let results = w.results("").await?;

    // Assert
    expect_that!(results.status, eq(StatusCode::OK));
    expect_that!(results.body["next_cursor"], eq(&Value::Null));
    let rows = results.body["rows"].as_array().context("rows")?;
    expect_that!(rows.len(), eq(2));
    expect_that!(rows[0]["run_id"], eq(&json!(second_run)));
    expect_that!(rows[0]["item"]["title"], eq(&json!("Second note")));
    expect_that!(rows[0]["item"]["stage"], eq(&json!("with_context")));
    expect_that!(rows[0]["goal"], eq(&json!("with_context")));
    expect_that!(rows[0]["reached"], eq(&json!("with_context")));
    expect_that!(rows[0]["result"], eq(&json!("Opened the pull request")));
    expect_that!(rows[0]["links"], eq(&json!(["https://github.com/acme/app/pull/7"])));
    expect_that!(rows[0]["day_label"], eq(&json!("today")));
    expect_that!(rows[0]["day"].as_str().map(str::len), some(eq(10)));
    expect_that!(rows[0]["time"].as_str().map(str::len), some(eq(5)));
    expect_that!(rows[0]["heap"]["id"], eq(&json!(w.heap)));
    expect_that!(rows[1]["run_id"], eq(&json!(first_run)));
    expect_that!(rows[1]["item"]["key"], eq(&json!(first)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn results_page_by_cursor_and_stop_at_thirty_days() -> Result<()> {
    // Arrange: a run that ended just over 30 days ago, one just under, and three this week.
    let w = world().await?;
    let (_, old) = w.reached_note("Old note").await?;
    let (_, edge) = w.reached_note("Edge note").await?;
    let mut runs = vec![edge.clone()];
    for title in ["A", "B", "C"] {
        w.app.clock.advance(Duration::from_minutes(1));
        runs.push(w.reached_note(title).await?.1);
    }

    let now = w.app.clock.now();
    w.app
        .set_run_ended_at(
            &old,
            now.checked_sub(Duration::from_days(30)).context("range")?.as_millis() - 1,
        )
        .await?;
    w.app
        .set_run_ended_at(
            &edge,
            now.checked_sub(Duration::from_days(30)).context("range")?.as_millis() + 1,
        )
        .await?;
    // Act
    let pages = w.app.results_in_pages(&w.user_id().await?, 2).await?;
    let first = w.results("").await?;
    let bad = w.results("?cursor=nonsense").await?;

    // Assert
    expect_that!(
        pages,
        eq(&vec![
            vec![runs[3].clone(), runs[2].clone()],
            vec![runs[1].clone(), runs[0].clone()]
        ])
    );
    expect_that!(first.body["rows"].as_array().map(Vec::len), some(eq(4)));
    expect_that!(first.body["rows"].to_string().contains(&old), eq(false));
    expect_that!(bad.status, eq(StatusCode::BAD_REQUEST));
    let signed_out = w.app.send(Method::GET, "/api/v1/me/results", None, None).await?;
    expect_that!(signed_out.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn results_leave_out_heaps_the_caller_cannot_view_and_read_an_unrecorded_reach_as_null() -> Result<()> {
    // Arrange: a run in Bob's own heap, and one of Ada's from before reaches were recorded.
    let w = world().await?;
    let (bob_session, bob_heap) = w.app.register("bob@example.test").await?;
    let bobs = w
        .app
        .new_item(
            &bob_session,
            &bob_heap,
            json!({ "title": "Bob's note", "description": "Why" }),
        )
        .await?;
    w.send_turn_as(&bob_session, &bob_heap, key_of(&bobs)).await?;
    let claimed = w
        .app
        .send(
            Method::POST,
            "/api/v1/work/runs",
            Some(&bob_session),
            Some(json!({ "heaps": [w.heap_ref(&bob_heap).await?] })),
        )
        .await?;
    let bobs_run = claimed.body["run"]["id"].as_str().context("run")?.to_owned();
    w.app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{bobs_run}/reached"),
            Some(&bob_session),
            Some(json!({ "result": "Done" })),
        )
        .await?;
    let (_, old) = w.reached_note("Ada's note").await?;
    w.app.clear_run_reached(&old).await?;

    // Act
    let results = w.results("").await?;

    // Assert
    let rows = results.body["rows"].as_array().context("rows")?;
    expect_that!(rows.len(), eq(1));
    expect_that!(rows[0]["run_id"], eq(&json!(old)));
    expect_that!(rows[0]["reached"], eq(&Value::Null));
    Ok(())
}

impl World {
    async fn send_turn_as(&self, session: &str, heap: &str, key: &str) -> Result<()> {
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{}/turn", self.item_path_in(heap, key).await?),
                Some(session),
                Some(json!({ "reach": "one" })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        Ok(())
    }
}

#[gtest]
#[tokio::test]
async fn an_item_an_agent_reached_is_a_decision_until_it_is_sent_on_started_closed_or_set_back() -> Result<()> {
    // Arrange: four notes reached by agents.
    let w = world().await?;
    let (sent_on, _) = w.reached_note("Sent on").await?;
    let (started, _) = w.reached_note("Started").await?;
    let (closed, _) = w.reached_note("Closed").await?;
    let (set_back, _) = w.reached_note("Set back").await?;
    let (waiting, _) = w.reached_note("Still waiting").await?;

    // Act
    let before = w.decisions().await?;
    w.send_turn(&w.heap, &sent_on).await?;
    w.edit(&started, json!({ "state": "in_progress" })).await?;
    w.edit(&closed, json!({ "state": "wont_do" })).await?;
    w.edit(&set_back, json!({ "stage": "as_written" })).await?;
    let after = w.decisions().await?;

    // Assert
    expect_that!(before["count"], eq(&json!(5)));
    expect_that!(after["count"], eq(&json!(1)));
    expect_that!(after["rows"][0]["item"]["key"], eq(&json!(waiting)));
    expect_that!(after["rows"][0]["item"]["stage"], eq(&json!("with_context")));
    expect_that!(after["rows"][0]["item"]["kind"], eq(&json!("note")));
    expect_that!(after["rows"][0]["heap"]["id"], eq(&json!(w.heap)));
    expect_that!(after["rows"][0]["arrived"], eq(&json!("today")));
    expect_that!(after["rows"][0]["became_label"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_the_person_reached_is_not_a_decision() -> Result<()> {
    // Arrange: a note sent to an agent, whose goal the person meets by setting the stage.
    let w = world().await?;
    let note = w.add("Met by the person").await?;
    w.send_turn(&w.heap, key_of(&note)).await?;

    // Act
    w.edit(key_of(&note), json!({ "stage": "with_context" })).await?;
    let decisions = w.decisions().await?;

    // Assert
    expect_that!(decisions, eq(&json!({ "count": 0, "rows": [] })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_that_became_items_is_one_decision_until_a_task_is_started() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (note, task) = w.became_note("Certs are a mess").await?;

    // Act
    let before = w.decisions().await?;
    w.edit(key_of(&task), json!({ "state": "in_progress" })).await?;
    let after = w.decisions().await?;

    // Assert
    expect_that!(before["count"], eq(&json!(1)));
    expect_that!(before["rows"][0]["item"]["key"], eq(&json!(note)));
    expect_that!(before["rows"][0]["became_label"], eq(&json!("made a task")));
    expect_that!(after, eq(&json!({ "count": 0, "rows": [] })));
    // The history still holds the run.
    let results = w.results("").await?;
    expect_that!(results.body["rows"][0]["reached"], eq(&json!("became")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn decisions_are_ordered_by_score_times_heap_priority_and_the_cap_keeps_the_count() -> Result<()> {
    // Arrange: a critical note in the personal heap, an expected one in the shared heap, which the person values more.
    let w = world().await?;
    let (_team, shared) = w.app.shared_heap(&w.session).await?;
    let critical = w.add_in(&w.heap, "Critical, personal", "critical").await?;
    w.reach_in(&w.heap, key_of(&critical)).await?;
    let idea = w.add_in(&shared, "Idea, shared", "expected").await?;
    w.reach_in(&shared, key_of(&idea)).await?;

    // Act
    let by_score = w.decisions().await?;
    w.app.set_priority(&w.session, &shared, 10).await?;
    let by_priority = w.decisions().await?;
    let capped = w.app.decisions_capped(&w.user_id().await?, 1).await?;

    // Assert
    expect_that!(
        titles(&by_score["rows"]),
        elements_are![eq(&"Critical, personal"), eq(&"Idea, shared")]
    );
    expect_that!(
        titles(&by_priority["rows"]),
        elements_are![eq(&"Idea, shared"), eq(&"Critical, personal")]
    );
    expect_that!(capped, eq((2, 1)));
    Ok(())
}
