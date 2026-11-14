//! Splitting: a note becomes several items. Items made from a note keep a link to it, `became` closes the note, and in a
//! run it also reaches the turn.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

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

fn code(response: &TestResponse) -> Option<&str> {
    response.body["code"].as_str()
}

fn id(item: &Value) -> Value {
    item["id"].clone()
}

impl World {
    async fn items_path(&self) -> Result<String> {
        Ok(format!("{}/items", self.app.heap_url(&self.heap).await?))
    }

    async fn item_path(&self, key: &str) -> Result<String> {
        Ok(format!("{}/{key}", self.items_path().await?))
    }

    /// Adds a note with a context, set at the stage "with context", and returns it.
    async fn add_note(&self, title: &str, need: &str) -> Result<Value> {
        let note = self
            .app
            .new_item(
                &self.session,
                &self.heap,
                json!({ "title": title, "need": need, "description": "Everything is a mess." }),
            )
            .await?;
        let key = note["key"].as_str().context("key")?;
        let set = self.edit(key, json!({ "stage": "with_context" })).await?;
        anyhow::ensure!(set.status == StatusCode::OK, "stage failed: {}", set.body);
        Ok(set.body)
    }

    /// Adds a note, sends it to an agent and claims it. Returns the note and the run's ID.
    async fn note_in_a_run(&self, need: &str) -> Result<(Value, String)> {
        self.note_in_a_run_reaching(need, "one").await
    }

    /// Like `note_in_a_run`, with the note's turn sent to `reach`.
    async fn note_in_a_run_reaching(&self, need: &str, reach: &str) -> Result<(Value, String)> {
        let note = self.add_note("Certs are a mess", need).await?;
        self.send_turn_reaching(note["key"].as_str().context("key")?, reach)
            .await?;
        let run = self.claimed().await?;
        Ok((note, run))
    }

    async fn send_turn(&self, key: &str) -> Result<()> {
        self.send_turn_reaching(key, "one").await
    }

    async fn send_turn_reaching(&self, key: &str, reach: &str) -> Result<()> {
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{}/turn", self.item_path(key).await?),
                Some(&self.session),
                Some(json!({ "reach": reach })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        Ok(())
    }

    async fn claimed(&self) -> Result<String> {
        let heap_ref = self.heap_ref().await?;
        let claimed = self
            .app
            .send(
                Method::POST,
                "/api/v1/work/runs",
                Some(&self.session),
                Some(json!({ "heaps": [heap_ref] })),
            )
            .await?;
        anyhow::ensure!(claimed.status == StatusCode::CREATED, "claim failed: {}", claimed.body);
        Ok(claimed.body["run"]["id"].as_str().context("no run")?.to_owned())
    }

    async fn heap_ref(&self) -> Result<String> {
        let url = self.app.heap_url(&self.heap).await?;
        Ok(url
            .strip_prefix("/api/v1/teams/")
            .context("heap url")?
            .replace("/heaps/", "/"))
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

    /// A request made in `run`, as the CLI makes it.
    async fn in_run(&self, run: &str, method: Method, path: &str, body: Option<Value>) -> Result<TestResponse> {
        self.app
            .send_with(method, path, Some(&self.session), body, &[CSRF, ("upheap-run", run)])
            .await
    }

    /// Makes a task from `note` as the person.
    async fn split(&self, note: &Value, title: &str, extra: Value) -> Result<TestResponse> {
        let mut body = json!({
            "title": title,
            "need": "expected",
            "kind": "task",
            "origin": { "item_id": id(note), "age": "keep" },
        });
        if let (Value::Object(body), Value::Object(extra)) = (&mut body, extra) {
            body.extend(extra);
        }
        self.app
            .send(Method::POST, &self.items_path().await?, Some(&self.session), Some(body))
            .await
    }

    /// Makes a task from `note` in `run` with a context, and returns the response.
    async fn split_in_run(&self, run: &str, note: &Value, title: &str, extra: Value) -> Result<TestResponse> {
        let mut body = json!({
            "title": title,
            "need": "expected",
            "kind": "task",
            "description": "Done when: it works.",
            "origin": { "item_id": id(note), "age": "keep" },
        });
        if let (Value::Object(body), Value::Object(extra)) = (&mut body, extra) {
            body.extend(extra);
        }
        self.in_run(run, Method::POST, &self.items_path().await?, Some(body))
            .await
    }

    async fn add_step_in_run(&self, run: &str, key: &str) -> Result<TestResponse> {
        self.in_run(
            run,
            Method::POST,
            &format!("{}/steps", self.item_path(key).await?),
            Some(json!({ "title": "Do it" })),
        )
        .await
    }

    async fn became(&self, key: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("{}/became", self.item_path(key).await?),
                Some(&self.session),
                None,
            )
            .await
    }

    async fn became_in_run(&self, run: &str, key: &str) -> Result<TestResponse> {
        self.in_run(
            run,
            Method::POST,
            &format!("{}/became", self.item_path(key).await?),
            None,
        )
        .await
    }

    async fn reached(&self, run: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("/api/v1/work/runs/{run}/reached"),
                Some(&self.session),
                Some(json!({ "result": "Done" })),
            )
            .await
    }

    async fn waiting(&self) -> Result<Value> {
        let waiting = self
            .app
            .send(Method::GET, "/api/v1/me/waiting", Some(&self.session), None)
            .await?;
        Ok(waiting.body)
    }
}

fn key_of(item: &Value) -> &str {
    item["key"].as_str().unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn a_run_splits_a_note_into_two_tasks_and_the_note_became() -> Result<()> {
    // Arrange: a note with a context, sent on, and claimed.
    let w = world().await?;
    let (note, run) = w.note_in_a_run("critical").await?;
    w.app.clock.advance(Duration::from_minutes(10));

    // Act: task A, then task B blocked by A, each with a step; then the note becomes.
    let a = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    let a_stepped = w.add_step_in_run(&run, key_of(&a.body)).await?;
    let b = w
        .split_in_run(
            &run,
            &note,
            "Rotate production",
            json!({ "blocked_by": id(&a.body), "need": "critical" }),
        )
        .await?;
    let b_stepped = w.add_step_in_run(&run, key_of(&b.body)).await?;
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert
    expect_that!(a.status, eq(StatusCode::CREATED));
    expect_that!(a_stepped.status, eq(StatusCode::CREATED));
    expect_that!(b.status, eq(StatusCode::CREATED));
    expect_that!(b_stepped.status, eq(StatusCode::CREATED));
    expect_that!(b.body["state"], eq(&json!("blocked")));
    expect_that!(b.body["blocked_by"]["key"], eq(&a.body["key"]));
    expect_that!(a.body["origin"]["key"], eq(&note["key"]));
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(became.body["state"], eq(&json!("became")));
    expect_that!(became.body["turn"], eq(&Value::Null));
    expect_that!(became.body["became"].as_array().map(Vec::len), some(eq(2)));
    // The age counts from the note's creation: its stored arrival is the note's `created_at`.
    let a_id = a.body["id"].as_str().context("id")?;
    expect_that!(
        w.app.item_arrives_at(a_id).await?,
        some(eq(note["created_at"].as_i64().context("created_at")?))
    );

    // Assert: both tasks are defined; the run and the turn ended as reached.
    let a_now = w.get(key_of(&a.body)).await?;
    let b_now = w.get(key_of(&b.body)).await?;
    expect_that!(a_now["stage"], eq(&json!("defined")));
    expect_that!(b_now["stage"], eq(&json!("defined")));
    let note_now = w.get(key_of(&note)).await?;
    expect_that!(note_now["became"][0]["key"], eq(&a.body["key"]));
    expect_that!(note_now["became"][0]["state"], eq(&json!("open")));
    expect_that!(note_now["became"][1]["state"], eq(&json!("blocked")));
    expect_that!(note_now["became"][1]["kind"], eq(&json!("task")));
    expect_that!(note_now["became"][0]["heap"]["id"], eq(&json!(w.heap.as_str())));
    let runs = w.app.audit_runs(note["id"].as_str().context("id")?).await?;
    expect_that!(runs.iter().any(|(_, run)| run.is_some()), eq(true));
    let queue = w
        .app
        .send(
            Method::GET,
            &format!("/api/v1/work/next?heaps={}", w.heap_ref().await?),
            Some(&w.session),
            None,
        )
        .await?;
    expect_that!(queue.body["item"], eq(&Value::Null));

    // Act: the person finishes A.
    let done = w.edit(key_of(&a.body), json!({ "state": "done" })).await?;

    // Assert: B is open.
    expect_that!(done.status, eq(StatusCode::OK));
    expect_that!(w.get(key_of(&b.body)).await?["state"], eq(&json!("open")));
    Ok(())
}

/// A note sent to `reach` and split in its run into task A, task B blocked by A (both `--from` the note) and a
/// follow-up C (no origin), then became. Returns the world, the note and the three tasks.
async fn split_and_became(reach: &str) -> Result<(World, Value, Value, Value, Value)> {
    let w = world().await?;
    let (note, run) = w.note_in_a_run_reaching("critical", reach).await?;
    let a = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&a.body)).await?;
    let b = w
        .split_in_run(
            &run,
            &note,
            "Rotate production",
            json!({ "blocked_by": id(&a.body), "need": "critical" }),
        )
        .await?;
    w.add_step_in_run(&run, key_of(&b.body)).await?;
    let c = w
        .in_run(
            &run,
            Method::POST,
            &w.items_path().await?,
            Some(json!({
                "title": "Check the alarms",
                "need": "expected",
                "kind": "task",
                "description": "Done when: they fire.",
            })),
        )
        .await?;
    anyhow::ensure!(c.status == StatusCode::CREATED, "follow-up failed: {}", c.body);
    w.add_step_in_run(&run, key_of(&c.body)).await?;
    let became = w.became_in_run(&run, key_of(&note)).await?;
    anyhow::ensure!(became.status == StatusCode::OK, "became failed: {}", became.body);
    Ok((w, note, a.body, b.body, c.body))
}

#[gtest]
#[tokio::test]
async fn a_note_sent_all_the_way_passes_its_turn_to_the_tasks_split_from_it() -> Result<()> {
    // Arrange, act: the note sent all the way is split and became.
    let (w, note, a, b, c) = split_and_became("all_the_way").await?;

    // Assert: A and B have a turn all the way to done; the follow-up C has none.
    for task in [&a, &b] {
        let turn = &w.get(key_of(task)).await?["turn"];
        expect_that!(turn["reach"], eq(&json!("all_the_way")));
        expect_that!(turn["goal"], eq(&json!("done")));
    }
    expect_that!(w.get(key_of(&c)).await?["turn"], eq(&Value::Null));

    // Assert: no turn stays on the note.
    expect_that!(w.get(key_of(&note)).await?["turn"], eq(&Value::Null));

    // Assert: B is blocked, so A is next.
    let next_path = format!("/api/v1/work/next?heaps={}", w.heap_ref().await?);
    let next = w.app.send(Method::GET, &next_path, Some(&w.session), None).await?;
    expect_that!(next.body["item"]["key"], eq(&a["key"]));

    // Act: the person closes A as done.
    let done = w.edit(key_of(&a), json!({ "state": "done" })).await?;

    // Assert: B is freed, keeps its turn, and is next.
    expect_that!(done.status, eq(StatusCode::OK));
    let b_now = w.get(key_of(&b)).await?;
    expect_that!(b_now["state"], eq(&json!("open")));
    expect_that!(b_now["turn"]["reach"], eq(&json!("all_the_way")));
    let next = w.app.send(Method::GET, &next_path, Some(&w.session), None).await?;
    expect_that!(next.body["item"]["key"], eq(&b["key"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn became_ends_the_turn_the_person_sent_on_a_split_task_and_passes_none_on() -> Result<()> {
    // Arrange: a note sent all the way; its run makes task A, and the person sends A on, one state, before it became.
    let w = world().await?;
    let (note, run) = w.note_in_a_run_reaching("critical", "all_the_way").await?;
    let a = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&a.body)).await?;
    w.send_turn(key_of(&a.body)).await?;

    // Act
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert: defined meets A's turn, so it ended as reached, and A gets no turn from the note.
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(w.get(key_of(&a.body)).await?["turn"], eq(&Value::Null));
    let entries = w.app.audit_entries(a.body["id"].as_str().context("id")?).await?;
    let reached = entries
        .iter()
        .find(|(action, _)| action == "turn_reached")
        .context("no turn_reached entry")?;
    expect_that!(reached.1["how"], eq(&json!("became")));
    expect_that!(reached.1["goal"], eq(&json!("defined")));
    expect_that!(
        entries.iter().filter(|(action, _)| action == "turn_opened").count(),
        eq(1)
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_split_task_s_turn_to_done_stays_open_through_became() -> Result<()> {
    // Arrange: a note sent all the way; its run makes task A, which the person defines and sends on, to done, then
    // sets back to with context, so became defines it again.
    let w = world().await?;
    let (note, run) = w.note_in_a_run_reaching("critical", "all_the_way").await?;
    let a = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&a.body)).await?;
    let defined = w.edit(key_of(&a.body), json!({ "stage": "defined" })).await?;
    anyhow::ensure!(defined.status == StatusCode::OK, "stage failed: {}", defined.body);
    w.send_turn(key_of(&a.body)).await?;
    let lowered = w.edit(key_of(&a.body), json!({ "stage": "with_context" })).await?;
    anyhow::ensure!(lowered.status == StatusCode::OK, "stage failed: {}", lowered.body);

    // Act
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert
    expect_that!(became.status, eq(StatusCode::OK));
    let turn = &w.get(key_of(&a.body)).await?["turn"];
    expect_that!(turn["reach"], eq(&json!("one")));
    expect_that!(turn["goal"], eq(&json!("done")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_split_task_the_person_started_gets_no_turn() -> Result<()> {
    // Arrange: a note sent all the way; its run makes task A, and the person starts A before it became.
    let w = world().await?;
    let (note, run) = w.note_in_a_run_reaching("critical", "all_the_way").await?;
    let a = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&a.body)).await?;
    let started = w.edit(key_of(&a.body), json!({ "state": "in_progress" })).await?;
    anyhow::ensure!(started.status == StatusCode::OK, "start failed: {}", started.body);

    // Act
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(w.get(key_of(&a.body)).await?["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_sent_one_state_gives_its_split_tasks_no_turn() -> Result<()> {
    // Arrange, act
    let (w, _, a, b, _) = split_and_became("one").await?;

    // Assert
    expect_that!(w.get(key_of(&a)).await?["turn"], eq(&Value::Null));
    expect_that!(w.get(key_of(&b)).await?["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blocker_that_became_leaves_what_it_blocks_waiting_for_the_person() -> Result<()> {
    // Arrange: a note that something was made from, and a task blocked by that note.
    let w = world().await?;
    let note = w.add_note("Pick a host", "expected").await?;
    w.split(&note, "Compare hosts", json!({})).await?;
    let blocked = w.split(&note, "Ship the feed", json!({})).await?.body;
    let blocked = w
        .edit(
            key_of(&blocked),
            json!({ "state": "blocked", "blocked_by_item_id": id(&note) }),
        )
        .await?
        .body;

    // Act
    let became = w.became(key_of(&note)).await?;
    let after = w.get(key_of(&blocked)).await?;
    let waiting = w.waiting().await?;

    // Assert
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(after["state"], eq(&json!("blocked")));
    expect_that!(after["blocker_gone"]["how"], eq(&json!("became")));
    expect_that!(after["blocked_by"]["state"], eq(&json!("became")));
    expect_that!(after["waits_for_you"], eq(&json!(true)));
    expect_that!(waiting["count"], eq(&json!(1)));
    expect_that!(waiting["rows"][0]["kind"], eq(&json!("blocker_gone")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blocker_closed_as_wont_do_leaves_a_split_task_waiting() -> Result<()> {
    // Arrange: task B made from the note, blocked by task A made from it.
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let a = w.split(&note, "Rotate staging", json!({})).await?.body;
    let b = w
        .split(&note, "Rotate production", json!({ "blocked_by": id(&a) }))
        .await?
        .body;

    // Act
    let closed = w.edit(key_of(&a), json!({ "state": "wont_do" })).await?;
    let after = w.get(key_of(&b)).await?;

    // Assert
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(after["state"], eq(&json!("blocked")));
    expect_that!(after["blocker_gone"]["how"], eq(&json!("wont_do")));
    expect_that!(after["waits_for_you"], eq(&json!(true)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_that_became_reopens_within_the_week_and_keeps_its_links() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let a = w.split(&note, "Rotate staging", json!({})).await?.body;
    let b = w.split(&note, "Rotate production", json!({})).await?.body;
    w.became(key_of(&note)).await?;
    w.app.clock.advance(Duration::from_days(6));

    // Act
    let reopened = w.edit(key_of(&note), json!({ "state": "open" })).await?;
    let closed = w.became(key_of(&note)).await?;
    w.app.clock.advance(Duration::from_days(8));
    let late = w.edit(key_of(&note), json!({ "state": "open" })).await?;

    // Assert
    expect_that!(reopened.status, eq(StatusCode::OK));
    expect_that!(reopened.body["state"], eq(&json!("open")));
    expect_that!(reopened.body["became"][0]["key"], eq(&a["key"]));
    expect_that!(reopened.body["became"][1]["key"], eq(&b["key"]));
    expect_that!(w.get(key_of(&a)).await?["origin"]["key"], eq(&note["key"]));
    expect_that!(closed.body["state"], eq(&json!("became")));
    expect_that!(late.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&late), some(eq("reopen_window_expired")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_closed_list_carries_what_a_note_became() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    w.split(&note, "Rotate staging", json!({})).await?;
    w.became(key_of(&note)).await?;

    // Act
    let closed = w
        .app
        .send(
            Method::GET,
            &format!("{}/closed", w.items_path().await?),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(closed.body["items"][0]["state"], eq(&json!("became")));
    expect_that!(
        closed.body["items"][0]["became"][0]["title"],
        eq(&json!("Rotate staging"))
    );
    expect_that!(
        closed.body["items"][0]["allowed_actions"].to_string(),
        contains_substring("reopen")
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn became_needs_a_note_that_something_was_made_from_and_repeats_as_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let task = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "A task", "kind": "task" }))
        .await?;

    // Act
    let nothing_made = w.became(key_of(&note)).await?;
    let on_a_task = w.became(key_of(&task)).await?;
    w.split(&note, "Rotate staging", json!({})).await?;
    let first = w.became(key_of(&note)).await?;
    let again = w.became(key_of(&note)).await?;

    // Assert
    expect_that!(nothing_made.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&nothing_made), some(eq("nothing_made")));
    expect_that!(on_a_task.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&on_a_task), some(eq("became_needs_note")));
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body["version"], eq(&first.body["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_is_held_to_the_note_it_splits() -> Result<()> {
    // Arrange: a note of need "expected" in a run, and an older split task of another turn.
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let early = w.split(&note, "Made by the person", json!({})).await?.body;
    let other = w.add_note("Another note", "expected").await?;
    w.app.clock.advance(Duration::from_minutes(1));
    w.send_turn(key_of(&note)).await?;
    let run = w.claimed().await?;
    let items = w.items_path().await?;

    // Act
    let too_high = w
        .split_in_run(&run, &note, "Too needed", json!({ "need": "critical" }))
        .await?;
    let blocked_by_existing = w
        .split_in_run(&run, &note, "Waits", json!({ "blocked_by": id(&other) }))
        .await?;
    let blocked_by_earlier = w
        .split_in_run(&run, &note, "Waits too", json!({ "blocked_by": id(&early) }))
        .await?;
    let blocked_by_the_note = w
        .split_in_run(&run, &note, "Waits again", json!({ "blocked_by": id(&note) }))
        .await?;
    let no_origin = w
        .in_run(
            &run,
            Method::POST,
            &items,
            Some(json!({ "title": "Loose", "need": "idea" })),
        )
        .await?;
    let wrong_origin = w.split_in_run(&run, &other, "Not mine", json!({})).await?;
    let reminder = w
        .split_in_run(
            &run,
            &note,
            "Reminder",
            json!({ "kind": "reminder", "due": { "date": "2027-03-12", "firm": true } }),
        )
        .await?;

    // Assert
    expect_that!(too_high.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&too_high), some(eq("run_need_above_origin")));
    expect_that!(too_high.body["highest"], eq(&json!("expected")));
    for refused in [&blocked_by_existing, &blocked_by_earlier, &blocked_by_the_note] {
        expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
        expect_that!(code(refused), some(eq("run_blocker_not_made")));
    }
    expect_that!(no_origin.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&no_origin), some(eq("run_add_needs_context")));
    expect_that!(wrong_origin.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&wrong_origin), some(eq("run_wrong_item")));
    expect_that!(reminder.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&reminder), some(eq("run_add_reminder")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn became_in_a_run_waits_for_every_made_task_to_have_a_context_and_a_step() -> Result<()> {
    // Arrange: one task made without a step.
    let w = world().await?;
    let (note, run) = w.note_in_a_run("expected").await?;
    let task = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;

    // Act
    let refused = w.became_in_run(&run, key_of(&note)).await?;
    let note_after = w.get(key_of(&note)).await?;
    w.add_step_in_run(&run, key_of(&task.body)).await?;
    let accepted = w.became_in_run(&run, key_of(&note)).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&refused), some(eq("not_reached")));
    expect_that!(
        refused.body["title"],
        eq(&json!(format!(
            "Task {}, made from this item, needs a context and a step.",
            key_of(&task.body)
        )))
    );
    expect_that!(note_after["state"], eq(&json!("open")));
    expect_that!(note_after["turn"].is_object(), eq(true));
    expect_that!(accepted.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reached_after_became_is_nothing_to_do_and_a_second_became_is_run_over() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (note, run) = w.note_in_a_run("expected").await?;
    let task = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&task.body)).await?;
    w.became_in_run(&run, key_of(&note)).await?;

    // Act
    let reached = w.reached(&run).await?;
    let again = w.became_in_run(&run, key_of(&note)).await?;

    // Assert
    expect_that!(reached.status, eq(StatusCode::OK));
    expect_that!(reached.body["end"], eq(&json!("reached")));
    expect_that!(reached.body["result"].as_str(), some(starts_with("Became 1 item: ")));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&again), some(eq("run_over")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_person_splits_only_an_open_note_of_the_same_heap() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let task = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "A task", "kind": "task" }))
        .await?;
    let closed = w.add_note("Closed", "expected").await?;
    w.edit(key_of(&closed), json!({ "state": "wont_do" })).await?;

    // Act
    let from_task = w.split(&task, "From a task", json!({})).await?;
    let from_closed = w.split(&closed, "From a closed note", json!({})).await?;
    let reminder = w
        .split(
            &note,
            "Reminder",
            json!({ "kind": "reminder", "due": { "date": "2027-03-12", "firm": true } }),
        )
        .await?;
    let stranger = w
        .app
        .send(
            Method::POST,
            &w.items_path().await?,
            Some(&w.session),
            Some(json!({
                "title": "From nowhere", "need": "idea",
                "origin": { "item_id": "018f0000-0000-7000-8000-000000000000", "age": "new" },
            })),
        )
        .await?;
    let fresh = w
        .split(
            &note,
            "A fresh start",
            json!({ "origin": { "item_id": id(&note), "age": "new" } }),
        )
        .await?;

    // Assert
    expect_that!(from_task.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&from_task), some(eq("origin_not_a_note")));
    expect_that!(from_closed.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&from_closed), some(eq("origin_closed")));
    expect_that!(reminder.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(reminder.body["field"], eq(&json!("origin")));
    expect_that!(stranger.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&stranger), some(eq("origin_not_in_heap")));
    expect_that!(fresh.status, eq(StatusCode::CREATED));
    expect_that!(
        w.app.item_arrives_at(fresh.body["id"].as_str().context("id")?).await?,
        none()
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_origin_in_another_heap_is_refused() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let (_, other) = w.app.shared_heap(&w.session).await?;

    // Act
    let made = w
        .app
        .send(
            Method::POST,
            &format!("{}/items", w.app.heap_url(&other).await?),
            Some(&w.session),
            Some(json!({
                "title": "Elsewhere", "need": "idea",
                "origin": { "item_id": id(&note), "age": "keep" },
            })),
        )
        .await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&made), some(eq("origin_not_in_heap")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_patch_cannot_close_a_note_as_became() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    w.split(&note, "Rotate staging", json!({})).await?;

    // Act
    let patched = w.edit(key_of(&note), json!({ "state": "became" })).await?;

    // Assert
    expect_that!(patched.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&patched), some(eq("became_needs_split")));
    Ok(())
}

async fn set_prepare(w: &World) -> Result<()> {
    let path = w.app.heap_url(&w.heap).await?;
    let heap = w.app.send(Method::GET, &path, Some(&w.session), None).await?;
    let set = w
        .app
        .patch(
            &w.session,
            &path,
            &heap.body["version"],
            json!({ "agents_prepare": true }),
        )
        .await?;
    anyhow::ensure!(set.status == StatusCode::OK, "prepare failed: {}", set.body);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn in_a_prepare_heap_a_run_s_items_get_no_turn_and_a_persons_do() -> Result<()> {
    // Arrange: the note is prepared by an agent, which claims it.
    let w = world().await?;
    set_prepare(&w).await?;
    let note = w
        .app
        .new_item(
            &w.session,
            &w.heap,
            json!({ "title": "Certs are a mess", "need": "expected", "description": "Everything is a mess." }),
        )
        .await?;
    let run = w.claimed().await?;

    // Act
    let by_the_run = w.split_in_run(&run, &note, "Made by the agent", json!({})).await?;
    let by_the_person = w.split(&note, "Made by the person", json!({})).await?;

    // Assert
    expect_that!(by_the_run.status, eq(StatusCode::CREATED));
    expect_that!(by_the_run.body["turn"], eq(&Value::Null));
    expect_that!(by_the_person.status, eq(StatusCode::CREATED));
    expect_that!(by_the_person.body["turn"].is_object(), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_adds_steps_to_the_tasks_split_from_its_note_and_writes_nothing_else_on_them() -> Result<()> {
    // Arrange: a task made by the person before the turn, and one made by the run.
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let earlier = w.split(&note, "Made earlier", json!({})).await?.body;
    w.app.clock.advance(Duration::from_minutes(5));
    w.send_turn(key_of(&note)).await?;
    let run = w.claimed().await?;
    let made = w.split_in_run(&run, &note, "Made now", json!({})).await?.body;

    // Act
    let step_on_made = w.add_step_in_run(&run, key_of(&made)).await?;
    let step_on_earlier = w.add_step_in_run(&run, key_of(&earlier)).await?;
    let comment_on_made = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/comments", w.item_path(key_of(&made)).await?),
            Some(json!({ "body": "Noted." })),
        )
        .await?;
    let question_on_made = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/questions", w.item_path(key_of(&made)).await?),
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    let question_on_note = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/questions", w.item_path(key_of(&note)).await?),
            Some(json!({ "kind": "question", "text": "Which pieces?" })),
        )
        .await?;

    // Assert
    expect_that!(step_on_made.status, eq(StatusCode::CREATED));
    // A task the person split earlier needs a step too before the note can become.
    expect_that!(step_on_earlier.status, eq(StatusCode::CREATED));
    expect_that!(comment_on_made.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&comment_on_made), some(eq("run_wrong_item")));
    expect_that!(question_on_made.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&question_on_made), some(eq("run_wrong_item")));
    expect_that!(question_on_note.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_tried_again_blocks_on_a_task_the_failed_run_of_its_turn_made() -> Result<()> {
    // Arrange: the first run makes a task and fails; the turn is claimed again.
    let w = world().await?;
    let (note, first) = w.note_in_a_run("expected").await?;
    let made = w.split_in_run(&first, &note, "Rotate staging", json!({})).await?.body;
    let failed = w
        .app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{first}/finish"),
            Some(&w.session),
            Some(json!({ "failure": "The tool crashed" })),
        )
        .await?;
    anyhow::ensure!(failed.status == StatusCode::OK, "finish failed: {}", failed.body);
    let second = w.claimed().await?;

    // Act
    let chained = w
        .split_in_run(&second, &note, "Rotate production", json!({ "blocked_by": id(&made) }))
        .await?;

    // Assert
    expect_that!(chained.status, eq(StatusCode::CREATED));
    expect_that!(chained.body["state"], eq(&json!("blocked")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_follow_up_a_run_adds_without_from_is_not_what_the_note_became() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (note, run) = w.note_in_a_run("expected").await?;
    let items = w.items_path().await?;

    // Act: a follow-up task with no origin, at a need above the note's, then became.
    let follow_up = w
        .in_run(
            &run,
            Method::POST,
            &items,
            Some(json!({ "title": "Audit the CA", "need": "critical", "kind": "task", "description": "Later." })),
        )
        .await?;
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert: it links back to the note, but the note became nothing.
    expect_that!(follow_up.status, eq(StatusCode::CREATED));
    expect_that!(follow_up.body["origin"]["key"], eq(&note["key"]));
    expect_that!(became.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&became), some(eq("nothing_made")));
    expect_that!(w.get(key_of(&note)).await?["became"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_tried_again_adds_steps_to_the_tasks_the_failed_run_of_its_turn_split() -> Result<()> {
    // Arrange: the first run makes a task without its step and fails.
    let w = world().await?;
    let (note, first) = w.note_in_a_run("expected").await?;
    let made = w.split_in_run(&first, &note, "Rotate staging", json!({})).await?.body;
    let failed = w
        .app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{first}/finish"),
            Some(&w.session),
            Some(json!({ "failure": "The tool crashed" })),
        )
        .await?;
    anyhow::ensure!(failed.status == StatusCode::OK, "finish failed: {}", failed.body);
    let second = w.claimed().await?;

    // Act
    let stepped = w.add_step_in_run(&second, key_of(&made)).await?;
    let became = w.became_in_run(&second, key_of(&note)).await?;

    // Assert
    expect_that!(stepped.status, eq(StatusCode::CREATED));
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(became.body["state"], eq(&json!("became")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn became_ends_the_prepare_turn_of_a_task_the_person_split_and_passes_the_note_s_turn_on() -> Result<()> {
    // Arrange: a note sent all the way and claimed; then the heap prepares new items, so the task the person makes
    // from the note gets a prepare turn to defined. The run adds its step.
    let w = world().await?;
    let (note, run) = w.note_in_a_run_reaching("critical", "all_the_way").await?;
    set_prepare(&w).await?;
    let task = w
        .split(
            &note,
            "Rotate staging",
            json!({ "description": "Done when: it works." }),
        )
        .await?;
    anyhow::ensure!(task.status == StatusCode::CREATED, "split failed: {}", task.body);
    expect_that!(task.body["turn"]["goal"], eq(&json!("defined")));
    w.add_step_in_run(&run, key_of(&task.body)).await?;

    // Act
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert: the prepare turn ended as reached, and the task has the note's turn, all the way to done.
    expect_that!(became.status, eq(StatusCode::OK));
    let entries = w.app.audit_entries(task.body["id"].as_str().context("id")?).await?;
    let reached = entries
        .iter()
        .find(|(action, _)| action == "turn_reached")
        .context("no turn_reached entry")?;
    expect_that!(reached.1["how"], eq(&json!("became")));
    expect_that!(reached.1["goal"], eq(&json!("defined")));
    let turn = &w.get(key_of(&task.body)).await?["turn"];
    expect_that!(turn["reach"], eq(&json!("all_the_way")));
    expect_that!(turn["goal"], eq(&json!("done")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn became_ends_the_prepare_turn_of_a_task_the_person_split_from_a_note_sent_one_state() -> Result<()> {
    // Arrange: as above, with the note sent one state.
    let w = world().await?;
    let (note, run) = w.note_in_a_run("critical").await?;
    set_prepare(&w).await?;
    let task = w
        .split(
            &note,
            "Rotate staging",
            json!({ "description": "Done when: it works." }),
        )
        .await?;
    anyhow::ensure!(task.status == StatusCode::CREATED, "split failed: {}", task.body);
    w.add_step_in_run(&run, key_of(&task.body)).await?;

    // Act
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert: the prepare turn ended as reached, and no turn is left on the task.
    expect_that!(became.status, eq(StatusCode::OK));
    let entries = w.app.audit_entries(task.body["id"].as_str().context("id")?).await?;
    expect_that!(
        entries
            .iter()
            .any(|(action, diff)| action == "turn_reached" && diff["how"] == json!("became")),
        eq(true)
    );
    expect_that!(w.get(key_of(&task.body)).await?["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_split_task_the_person_sent_all_the_way_goes_on_with_its_own_turn() -> Result<()> {
    // Arrange: a note sent all the way; its run makes task A, and the person sends A all the way too.
    let w = world().await?;
    let (note, run) = w.note_in_a_run_reaching("critical", "all_the_way").await?;
    let a = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&a.body)).await?;
    w.send_turn_reaching(key_of(&a.body), "all_the_way").await?;
    let sent = w.get(key_of(&a.body)).await?["turn"].clone();

    // Act
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert: A's turn to defined ended as reached and its own follow-on goes to done; the note passes none on.
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(sent["goal"], eq(&json!("defined")));
    expect_that!(w.get(key_of(&a.body)).await?["turn"]["goal"], eq(&json!("done")));
    let entries = w.app.audit_entries(a.body["id"].as_str().context("id")?).await?;
    expect_that!(
        entries.iter().filter(|(action, _)| action == "turn_opened").count(),
        eq(2)
    );
    Ok(())
}

impl World {
    async fn task(&self, title: &str) -> Result<Value> {
        self.app
            .new_item(&self.session, &self.heap, json!({ "title": title, "kind": "task" }))
            .await
    }

    async fn to_step(&self, note: &Value, task: &Value) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("{}/to-step", self.item_path(key_of(note)).await?),
                Some(&self.session),
                Some(json!({ "task_id": id(task) })),
            )
            .await
    }

    async fn to_step_in_run(&self, run: &str, note: &Value, task: &Value) -> Result<TestResponse> {
        self.in_run(
            run,
            Method::POST,
            &format!("{}/to-step", self.item_path(key_of(note)).await?),
            Some(json!({ "task_id": id(task) })),
        )
        .await
    }

    async fn ask(&self, key: &str) -> Result<()> {
        let asked = self
            .app
            .send(
                Method::POST,
                &format!("{}/questions", self.item_path(key).await?),
                Some(&self.session),
                Some(json!({ "kind": "question", "text": "Which CA?" })),
            )
            .await?;
        anyhow::ensure!(asked.status == StatusCode::CREATED, "ask failed: {}", asked.body);
        Ok(())
    }
}

#[gtest]
#[tokio::test]
async fn a_person_makes_a_note_a_step_of_a_task() -> Result<()> {
    // Arrange: a note with a context, an open turn and an open question, and a task with a step.
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let task = w.task("Rotate the certs").await?;
    w.app
        .send(
            Method::POST,
            &format!("{}/steps", w.item_path(key_of(&task)).await?),
            Some(&w.session),
            Some(json!({ "title": "Order it" })),
        )
        .await?;
    w.send_turn(key_of(&note)).await?;
    w.ask(key_of(&note)).await?;

    // Act
    let moved = w.to_step(&note, &task).await?;

    // Assert: the note became, naming the step and its task, and nothing is left open on it.
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["state"], eq(&json!("became")));
    expect_that!(moved.body["turn"], eq(&Value::Null));
    expect_that!(moved.body["questions"][0]["state"], eq(&json!("withdrawn")));
    expect_that!(moved.body["became"], eq(&json!([])));
    expect_that!(moved.body["became_steps"][0]["number"], eq(&json!(2)));
    expect_that!(moved.body["became_steps"][0]["title"], eq(&json!("Certs are a mess")));
    expect_that!(moved.body["became_steps"][0]["state"], eq(&json!("open")));
    expect_that!(moved.body["became_steps"][0]["task"]["key"], eq(&task["key"]));
    expect_that!(
        moved.body["became_steps"][0]["task"]["heap"]["id"],
        eq(&json!(w.heap.as_str()))
    );

    // Assert: the task has the step at the end, with the note's context and its origin; its own state is unchanged.
    let task_now = w.get(key_of(&task)).await?;
    expect_that!(task_now["steps"][1]["title"], eq(&json!("Certs are a mess")));
    expect_that!(task_now["steps"][1]["context"], eq(&json!("Everything is a mess.")));
    expect_that!(task_now["steps"][1]["origin"]["key"], eq(&note["key"]));
    expect_that!(task_now["steps"][0]["origin"], eq(&Value::Null));
    expect_that!(task_now["state"], eq(&json!("open")));
    expect_that!(task_now["version"], eq(&task["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_becomes_a_step_only_of_an_open_task_of_its_heap() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let (_, other_heap) = w.app.shared_heap(&w.session).await?;
    let elsewhere = w
        .app
        .new_item(&w.session, &other_heap, json!({ "title": "Elsewhere", "kind": "task" }))
        .await?;
    let a_note = w.add_note("Just a note", "expected").await?;
    let closed_task = w.task("Closed task").await?;
    w.edit(key_of(&closed_task), json!({ "state": "done" })).await?;
    let task = w.task("Rotate the certs").await?;
    let unknown = json!({ "id": "018f0000-0000-7000-8000-000000000000" });

    // Act
    let in_other_heap = w.to_step(&note, &elsewhere).await?;
    let unknown_task = w.to_step(&note, &unknown).await?;
    let to_a_note = w.to_step(&note, &a_note).await?;
    let to_a_closed_task = w.to_step(&note, &closed_task).await?;
    let from_a_task = w.to_step(&task, &task).await?;
    let first = w.to_step(&note, &task).await?;
    let again = w.to_step(&note, &task).await?;
    let from_closed = w.to_step(&closed_task, &task).await?;

    // Assert
    expect_that!(in_other_heap.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&in_other_heap), some(eq("step_task_not_in_heap")));
    expect_that!(in_other_heap.body["field"], eq(&json!("task_id")));
    expect_that!(unknown_task.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&unknown_task), some(eq("step_task_not_in_heap")));
    expect_that!(to_a_note.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&to_a_note), some(eq("steps_need_task")));
    expect_that!(to_a_closed_task.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&to_a_closed_task), some(eq("item_closed")));
    expect_that!(from_a_task.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&from_a_task), some(eq("became_needs_note")));
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&again), some(eq("item_closed")));
    expect_that!(from_closed.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&from_closed), some(eq("became_needs_note")));
    // Only the one step was added, and the refused moves left the note open.
    expect_that!(
        w.get(key_of(&task)).await?["steps"].as_array().map(Vec::len),
        some(eq(1))
    );
    expect_that!(w.get(key_of(&a_note)).await?["state"], eq(&json!("open")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_note_is_not_made_a_step() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    w.edit(key_of(&note), json!({ "state": "wont_do" })).await?;
    let task = w.task("Rotate the certs").await?;

    // Act
    let refused = w.to_step(&note, &task).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&refused), some(eq("item_closed")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_closed_list_and_the_step_carry_where_a_note_became_a_step() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Certs are a mess", "expected").await?;
    let task = w.task("Rotate the certs").await?;
    w.to_step(&note, &task).await?;

    // Act
    let closed = w
        .app
        .send(
            Method::GET,
            &format!("{}/closed", w.items_path().await?),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(closed.body["items"][0]["became"], eq(&json!([])));
    expect_that!(
        closed.body["items"][0]["became_steps"][0]["task"]["title"],
        eq(&json!("Rotate the certs"))
    );
    // A list row doesn't carry them.
    let ranked = w
        .app
        .send(Method::GET, &w.items_path().await?, Some(&w.session), None)
        .await?;
    expect_that!(ranked.body["items"][0]["became_steps"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_makes_its_note_a_step_and_reaches_the_turn() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (note, run) = w.note_in_a_run("expected").await?;
    let task = w.task("Rotate the certs").await?;

    // Act
    let moved = w.to_step_in_run(&run, &note, &task).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["state"], eq(&json!("became")));
    expect_that!(moved.body["turn"], eq(&Value::Null));
    let note_now = w.get(key_of(&note)).await?;
    expect_that!(note_now["runs"][0]["end"], eq(&json!("reached")));
    expect_that!(
        note_now["runs"][0]["result"],
        eq(&json!(format!(
            "Became step {0}.1 \"Certs are a mess\" of {0} \"Rotate the certs\"",
            key_of(&task)
        )))
    );
    // The task has the step, and no turn opens on it.
    let task_now = w.get(key_of(&task)).await?;
    expect_that!(task_now["steps"][0]["origin"]["key"], eq(&note["key"]));
    expect_that!(task_now["turn"], eq(&Value::Null));
    expect_that!(task_now["version"], eq(&task["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_makes_only_its_own_note_a_step() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (_note, run) = w.note_in_a_run("expected").await?;
    let other = w.add_note("Another note", "expected").await?;
    let task = w.task("Rotate the certs").await?;

    // Act
    let refused = w.to_step_in_run(&run, &other, &task).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&refused), some(eq("run_wrong_item")));
    expect_that!(w.get(key_of(&task)).await?["steps"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_with_an_open_question_on_its_note_does_not_make_it_a_step() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (note, run) = w.note_in_a_run("expected").await?;
    let task = w.task("Rotate the certs").await?;
    w.ask(key_of(&note)).await?;

    // Act
    let refused = w.to_step_in_run(&run, &note, &task).await?;

    // Assert: nothing was written.
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&refused), some(eq("not_reached")));
    expect_that!(w.get(key_of(&task)).await?["steps"], eq(&json!([])));
    let note_now = w.get(key_of(&note)).await?;
    expect_that!(note_now["state"], eq(&json!("open")));
    expect_that!(note_now["turn"].is_object(), eq(true));
    Ok(())
}

impl World {
    async fn team_slug(&self) -> Result<String> {
        Ok(self.heap_ref().await?.split('/').next().unwrap_or_default().to_owned())
    }

    /// `/api/v1/teams/{team}/heaps`, the team's heaps.
    async fn heaps_path(&self) -> Result<String> {
        Ok(format!("/api/v1/teams/{}/heaps", self.team_slug().await?))
    }

    /// The path of the made `heap`, a heap body as the API returns it.
    async fn made_path(&self, heap: &Value) -> Result<String> {
        Ok(format!(
            "{}/{}",
            self.heaps_path().await?,
            heap["slug"].as_str().unwrap_or_default()
        ))
    }

    /// Makes a heap named `name` from `note` as the person, with a context, whatever the outcome.
    async fn make_heap_named(&self, note: &Value, name: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &self.heaps_path().await?,
                Some(&self.session),
                Some(json!({ "name": name, "context": "A move.", "origin_id": id(note) })),
            )
            .await
    }

    async fn make_heap(&self, note: &Value) -> Result<TestResponse> {
        self.make_heap_named(note, "Blog move").await
    }

    /// Makes a heap from `note` as the person, and returns it.
    async fn made_heap(&self, note: &Value, name: &str) -> Result<Value> {
        let made = self.make_heap_named(note, name).await?;
        anyhow::ensure!(made.status == StatusCode::CREATED, "make heap failed: {}", made.body);
        Ok(made.body)
    }

    /// Adds an item to the made `heap`, in `run` when there is one, made from `note` with a need and an age unless
    /// `body` says otherwise.
    async fn add_to_heap(
        &self,
        heap: &Value,
        run: Option<&str>,
        note: &Value,
        mut body: Value,
    ) -> Result<TestResponse> {
        if let Value::Object(fields) = &mut body {
            fields
                .entry("origin")
                .or_insert_with(|| json!({ "item_id": id(note), "age": "keep" }));
            fields.entry("need").or_insert(json!("expected"));
        }
        let path = format!("{}/items", self.made_path(heap).await?);
        match run {
            Some(run) => self.in_run(run, Method::POST, &path, Some(body)).await,
            None => {
                self.app
                    .send(Method::POST, &path, Some(&self.session), Some(body))
                    .await
            }
        }
    }

    async fn get_heap(&self, heap: &Value) -> Result<Value> {
        let read = self
            .app
            .send(Method::GET, &self.made_path(heap).await?, Some(&self.session), None)
            .await?;
        Ok(read.body)
    }

    async fn get_in(&self, heap: &Value, key: &str) -> Result<Value> {
        let path = format!("{}/items/{key}", self.made_path(heap).await?);
        Ok(self.app.send(Method::GET, &path, Some(&self.session), None).await?.body)
    }

    /// Makes a heap in `run` from `origin` (none when null) with the fields of `body`.
    async fn make_heap_in_run(&self, run: &str, origin: &Value, mut body: Value) -> Result<TestResponse> {
        if let Value::Object(fields) = &mut body {
            fields.entry("name").or_insert(json!("Blog move"));
            if !origin.is_null() {
                fields.entry("origin_id").or_insert(id(origin));
            }
        }
        self.in_run(run, Method::POST, &self.heaps_path().await?, Some(body))
            .await
    }
}

#[gtest]
#[tokio::test]
async fn a_person_makes_a_heap_from_a_note() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Move the blog", "expected").await?;

    // Act
    let made = w.make_heap(&note).await?;

    // Assert: the heap is in the note's team, with its context, prepare off and the note as its origin.
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(made.body["slug"], eq(&json!("blog-move")));
    expect_that!(made.body["context"], eq(&json!("A move.")));
    expect_that!(made.body["agents_prepare"], eq(&json!(false)));
    expect_that!(made.body["origin"]["key"], eq(&note["key"]));
    expect_that!(made.body["origin"]["heap"]["id"], eq(&json!(w.heap.as_str())));
    // The heap's own read says it too, and the note is as it was.
    let read = w.get_heap(&made.body).await?;
    expect_that!(read["origin"]["title"], eq(&json!("Move the blog")));
    let note_now = w.get(key_of(&note)).await?;
    expect_that!(note_now["state"], eq(&json!("open")));
    expect_that!(note_now["version"], eq(&note["version"]));
    expect_that!(note_now["became_heaps"][0]["slug"], eq(&json!("blog-move")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_is_made_only_from_an_open_note() -> Result<()> {
    // Arrange
    let w = world().await?;
    let task = w.task("Rotate the certs").await?;
    let closed = w.add_note("Closed", "expected").await?;
    w.edit(key_of(&closed), json!({ "state": "wont_do" })).await?;
    let missing = json!({ "id": "018f0000-0000-7000-8000-000000000000" });

    // Act
    let from_task = w.make_heap(&task).await?;
    let from_closed = w.make_heap(&closed).await?;
    let from_nothing = w.make_heap(&missing).await?;

    // Assert
    expect_that!(from_task.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&from_task), some(eq("origin_not_a_note")));
    expect_that!(from_closed.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&from_closed), some(eq("origin_closed")));
    expect_that!(from_nothing.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_are_added_from_the_note_to_its_heap_and_to_no_other() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Move the blog", "expected").await?;
    let heap = w.made_heap(&note, "Blog move").await?;
    let other_note = w.add_note("Another", "expected").await?;
    let other_heap = w.made_heap(&other_note, "Other").await?;
    let (_, unrelated) = w.app.shared_heap(&w.session).await?;

    // Act
    let added = w
        .add_to_heap(&heap, None, &note, json!({ "title": "Move posts", "kind": "task" }))
        .await?;
    let into_another_notes_heap = w
        .add_to_heap(&other_heap, None, &note, json!({ "title": "Wrong heap" }))
        .await?;
    let into_unrelated = w
        .app
        .send(
            Method::POST,
            &format!("{}/items", w.app.heap_url(&unrelated).await?),
            Some(&w.session),
            Some(json!({
                "title": "Elsewhere",
                "need": "expected",
                "origin": { "item_id": id(&note), "age": "keep" },
            })),
        )
        .await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::CREATED));
    expect_that!(added.body["origin"]["key"], eq(&note["key"]));
    expect_that!(into_another_notes_heap.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&into_another_notes_heap), some(eq("origin_not_in_heap")));
    expect_that!(into_unrelated.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&into_unrelated), some(eq("origin_not_in_heap")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_person_closes_the_note_as_became_once_its_heap_has_items() -> Result<()> {
    // Arrange
    let w = world().await?;
    let note = w.add_note("Move the blog", "expected").await?;
    let heap = w.made_heap(&note, "Blog move").await?;
    let empty = w.became(key_of(&note)).await?;
    w.add_to_heap(&heap, None, &note, json!({ "title": "Move posts", "kind": "task" }))
        .await?;

    // Act
    let became = w.became(key_of(&note)).await?;
    let closed = w
        .app
        .send(
            Method::GET,
            &format!("{}/closed", w.items_path().await?),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert: a heap with no items made nothing, then it counts.
    expect_that!(empty.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&empty), some(eq("nothing_made")));
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(became.body["state"], eq(&json!("became")));
    expect_that!(became.body["became_heaps"][0]["slug"], eq(&heap["slug"]));
    expect_that!(
        became.body["became_heaps"][0]["team_slug"],
        eq(&json!(w.team_slug().await?))
    );
    expect_that!(
        closed.body["items"][0]["became_heaps"][0]["name"],
        eq(&json!("Blog move"))
    );
    // A list row doesn't carry them.
    let ranked = w
        .app
        .send(Method::GET, &w.items_path().await?, Some(&w.session), None)
        .await?;
    expect_that!(ranked.body["items"][0]["became_heaps"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_makes_a_heap_from_its_note_with_a_context_and_only_that() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (note, run) = w.note_in_a_run("expected").await?;
    let other = w.add_note("Another note", "expected").await?;
    let own_heap = w.app.heap_url(&w.heap).await?;

    // Act
    let made = w.make_heap_in_run(&run, &note, json!({ "context": "A move." })).await?;
    let another = w
        .make_heap_in_run(&run, &other, json!({ "name": "Other", "context": "A move." }))
        .await?;
    let no_origin = w
        .make_heap_in_run(&run, &Value::Null, json!({ "name": "Free", "context": "A move." }))
        .await?;
    let no_context = w.make_heap_in_run(&run, &note, json!({ "name": "Bare" })).await?;
    let patched = w
        .in_run(&run, Method::PATCH, &own_heap, Some(json!({ "context": "Changed" })))
        .await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(made.body["origin"]["key"], eq(&note["key"]));
    expect_that!(made.body["context"], eq(&json!("A move.")));
    expect_that!(another.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&another), some(eq("run_wrong_item")));
    expect_that!(no_origin.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&no_origin), some(eq("run_forbidden")));
    expect_that!(no_context.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&no_context), some(eq("run_add_needs_context")));
    expect_that!(patched.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&patched), some(eq("run_forbidden")));
    // The run goes on: the note is still open, with its turn.
    let note_now = w.get(key_of(&note)).await?;
    expect_that!(note_now["state"], eq(&json!("open")));
    expect_that!(note_now["turn"].is_object(), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_runs_became_checks_the_heap_made_from_its_note_and_defines_its_tasks() -> Result<()> {
    // Arrange: a heap with no context, made by the person from a note sent all the way, and a run on the note. The
    // run splits a task into the note's own heap, so something was made.
    let w = world().await?;
    let note = w.add_note("Move the blog", "expected").await?;
    let made = w
        .app
        .send(
            Method::POST,
            &w.heaps_path().await?,
            Some(&w.session),
            Some(json!({ "name": "Blog move", "origin_id": id(&note) })),
        )
        .await?;
    expect_that!(made.status, eq(StatusCode::CREATED));
    let heap = made.body;
    w.send_turn_reaching(key_of(&note), "all_the_way").await?;
    let run = w.claimed().await?;
    let own = w.split_in_run(&run, &note, "Rotate staging", json!({})).await?;
    w.add_step_in_run(&run, key_of(&own.body)).await?;
    let message = |refused: &TestResponse| refused.body["title"].clone();

    // Act and assert: the heap has no context.
    let no_context = w.became_in_run(&run, key_of(&note)).await?;
    expect_that!(no_context.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&no_context), some(eq("not_reached")));
    expect_that!(
        message(&no_context),
        eq(&json!("Heap blog-move, made from this item, needs a context."))
    );

    // Act and assert: it has one now, and nothing made from the note is in it.
    let patched = w
        .app
        .patch(
            &w.session,
            &w.made_path(&heap).await?,
            &heap["version"],
            json!({ "context": "A move." }),
        )
        .await?;
    expect_that!(patched.status, eq(StatusCode::OK));
    let empty = w.became_in_run(&run, key_of(&note)).await?;
    expect_that!(code(&empty), some(eq("not_reached")));
    expect_that!(
        message(&empty),
        eq(&json!("Heap blog-move, made from this item, needs a task or a note."))
    );

    // Act and assert: a note without a context is in it.
    let bare = w
        .add_to_heap(&heap, None, &note, json!({ "title": "Old posts", "kind": "note" }))
        .await?;
    expect_that!(bare.status, eq(StatusCode::CREATED));
    let note_without_context = w.became_in_run(&run, key_of(&note)).await?;
    expect_that!(code(&note_without_context), some(eq("not_reached")));
    expect_that!(
        message(&note_without_context),
        eq(&json!(format!(
            "Note {}, made from this item, needs a context.",
            key_of(&bare.body)
        )))
    );
    let described = w
        .app
        .patch(
            &w.session,
            &format!("{}/items/{}", w.made_path(&heap).await?, key_of(&bare.body)),
            &bare.body["version"],
            json!({ "description": "Old ones too." }),
        )
        .await?;
    expect_that!(described.status, eq(StatusCode::OK));

    // Act and assert: a task in the heap needs its steps, like any made from the note.
    let task = w
        .add_to_heap(
            &heap,
            Some(&run),
            &note,
            json!({ "title": "Move posts", "kind": "task", "description": "Done when: moved." }),
        )
        .await?;
    expect_that!(task.status, eq(StatusCode::CREATED));
    let no_step = w.became_in_run(&run, key_of(&note)).await?;
    expect_that!(code(&no_step), some(eq("not_reached")));
    let step = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/items/{}/steps", w.made_path(&heap).await?, key_of(&task.body)),
            Some(json!({ "title": "Do it" })),
        )
        .await?;
    expect_that!(step.status, eq(StatusCode::CREATED));
    let became = w.became_in_run(&run, key_of(&note)).await?;

    // Assert: reached, the result names the heap and the item left in the note's own heap.
    expect_that!(became.status, eq(StatusCode::OK));
    expect_that!(became.body["state"], eq(&json!("became")));
    let note_now = w.get(key_of(&note)).await?;
    expect_that!(note_now["runs"][0]["end"], eq(&json!("reached")));
    expect_that!(
        note_now["runs"][0]["result"],
        eq(&json!(format!(
            "Became heap {}/blog-move \"Blog move\" (2 items) and 1 item: {} \"Rotate staging\"",
            w.team_slug().await?,
            key_of(&own.body)
        )))
    );
    // The heap's task is defined and has no turn, though the note went all the way; the note's own heap's task has.
    let task_now = w.get_in(&heap, key_of(&task.body)).await?;
    expect_that!(task_now["stage"], eq(&json!("defined")));
    expect_that!(task_now["turn"], eq(&Value::Null));
    let own_now = w.get(key_of(&own.body)).await?;
    expect_that!(own_now["stage"], eq(&json!("defined")));
    expect_that!(own_now["turn"].is_object(), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_queue_takes_items_from_a_heap_made_from_a_note_in_a_named_heap() -> Result<()> {
    // Arrange: a task in the made heap with a go-ahead turn.
    let w = world().await?;
    let note = w.add_note("Move the blog", "expected").await?;
    let heap = w.made_heap(&note, "Blog move").await?;
    let task = w
        .add_to_heap(
            &heap,
            None,
            &note,
            json!({ "title": "Move posts", "kind": "task", "description": "Done when: moved." }),
        )
        .await?;
    let sent = w
        .app
        .send(
            Method::POST,
            &format!("{}/items/{}/turn", w.made_path(&heap).await?, key_of(&task.body)),
            Some(&w.session),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    expect_that!(sent.status, eq(StatusCode::CREATED));

    // Act: only the note's heap is named.
    let next = w
        .app
        .send(
            Method::GET,
            &format!("/api/v1/work/next?heaps={}", w.heap_ref().await?),
            Some(&w.session),
            None,
        )
        .await?;
    let claimed = w
        .app
        .send(
            Method::POST,
            "/api/v1/work/runs",
            Some(&w.session),
            Some(json!({ "heaps": [w.heap_ref().await?] })),
        )
        .await?;

    // Assert
    expect_that!(next.body["item"]["key"], eq(&task.body["key"]));
    expect_that!(claimed.status, eq(StatusCode::CREATED));
    expect_that!(claimed.body["run"]["item"]["key"], eq(&task.body["key"]));
    expect_that!(claimed.body["run"]["item"]["heap"]["slug"], eq(&heap["slug"]));
    Ok(())
}
