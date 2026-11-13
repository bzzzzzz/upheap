//! The agent's queue and its runs: what `next` offers, claiming, what a run may write, how it ends, and a turn that
//! gets stuck.

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
    /// `team/heap` by slug, as the queue names it.
    heap_ref: String,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let heap_ref = heap_ref(&app, &heap).await?;
    Ok(World {
        app,
        session,
        heap,
        heap_ref,
    })
}

async fn heap_ref(app: &TestApp, heap: &str) -> Result<String> {
    let url = app.heap_url(heap).await?;
    Ok(url
        .strip_prefix("/api/v1/teams/")
        .context("heap url")?
        .replace("/heaps/", "/"))
}

impl World {
    async fn item_path(&self, key: &str) -> Result<String> {
        Ok(format!("{}/items/{key}", self.app.heap_url(&self.heap).await?))
    }

    /// Adds a note with `fields` and returns its key.
    async fn add(&self, title: &str, fields: Value) -> Result<String> {
        let mut body = json!({ "title": title });
        if let (Value::Object(body), Value::Object(fields)) = (&mut body, fields) {
            body.extend(fields);
        }
        let item = self.app.new_item(&self.session, &self.heap, body).await?;
        Ok(item["key"].as_str().context("no key")?.to_owned())
    }

    /// Adds a note with a context and sends it to an agent. Returns its key.
    async fn add_sent(&self, title: &str) -> Result<String> {
        let key = self.add(title, json!({})).await?;
        self.send_turn(&key).await?;
        Ok(key)
    }

    async fn send_turn(&self, key: &str) -> Result<()> {
        self.send_turn_with(key, json!({ "reach": "one" })).await
    }

    /// Sends the item to an agent, all the way, with a line.
    async fn send_all_the_way(&self, key: &str) -> Result<()> {
        self.send_turn_with(key, json!({ "reach": "all_the_way", "line": "Use the new CA." }))
            .await
    }

    async fn send_turn_with(&self, key: &str, body: Value) -> Result<()> {
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{}/turn", self.item_path(key).await?),
                Some(&self.session),
                Some(body),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Value> {
        let fetched = self
            .app
            .send(Method::GET, &self.item_path(key).await?, Some(&self.session), None)
            .await?;
        Ok(fetched.body)
    }

    /// Edits the item as the person.
    async fn edit(&self, key: &str, body: Value) -> Result<TestResponse> {
        let version = self.get(key).await?["version"].clone();
        self.app
            .patch(&self.session, &self.item_path(key).await?, &version, body)
            .await
    }

    async fn next(&self) -> Result<TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("/api/v1/work/next?heaps={}", self.heap_ref),
                Some(&self.session),
                None,
            )
            .await
    }

    async fn claim(&self) -> Result<TestResponse> {
        self.claim_as(&self.session).await
    }

    async fn claim_as(&self, session: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                "/api/v1/work/runs",
                Some(session),
                Some(json!({ "heaps": [self.heap_ref] })),
            )
            .await
    }

    /// Claims and returns the run's ID.
    async fn claimed(&self) -> Result<String> {
        let claimed = self.claim().await?;
        anyhow::ensure!(claimed.status == StatusCode::CREATED, "claim failed: {}", claimed.body);
        Ok(claimed.body["run"]["id"].as_str().context("no run")?.to_owned())
    }

    /// A request in `run`.
    async fn in_run(
        &self,
        run: &str,
        method: Method,
        path: &str,
        if_match: Option<&Value>,
        body: Option<Value>,
    ) -> Result<TestResponse> {
        let version = if_match.map(Value::to_string);
        let mut headers = vec![CSRF, ("upheap-run", run)];
        if let Some(version) = &version {
            headers.push(("if-match", version));
        }
        self.app
            .send_with(method, path, Some(&self.session), body, &headers)
            .await
    }

    /// Sets the item's context in `run`, with its current version.
    async fn write_context(&self, run: &str, key: &str, text: &str) -> Result<TestResponse> {
        let version = self.get(key).await?["version"].clone();
        self.in_run(
            run,
            Method::PATCH,
            &self.item_path(key).await?,
            Some(&version),
            Some(json!({ "description": text })),
        )
        .await
    }

    async fn run_call(&self, run: &str, action: &str, body: Option<Value>) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("/api/v1/work/runs/{run}/{action}"),
                Some(&self.session),
                body,
            )
            .await
    }

    async fn reached(&self, run: &str) -> Result<TestResponse> {
        self.run_call(run, "reached", Some(json!({ "result": "Added the context" })))
            .await
    }

    async fn finish(&self, run: &str, body: Value) -> Result<TestResponse> {
        self.run_call(run, "finish", Some(body)).await
    }

    /// Claims, finishes the run as failed and returns its ID.
    async fn fail_once(&self) -> Result<String> {
        let run = self.claimed().await?;
        let finished = self.finish(&run, json!({ "failure": "The tool crashed" })).await?;
        anyhow::ensure!(finished.status == StatusCode::OK, "finish failed: {}", finished.body);
        Ok(run)
    }

    async fn waiting(&self) -> Result<Value> {
        let waiting = self
            .app
            .send(Method::GET, "/api/v1/me/waiting", Some(&self.session), None)
            .await?;
        Ok(waiting.body)
    }
}

fn code(response: &TestResponse) -> Option<&str> {
    response.body["code"].as_str()
}

fn actions(value: &Value) -> Vec<&str> {
    value["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn next_offers_the_highest_scored_item_and_writes_nothing() -> Result<()> {
    // Arrange: an old note, and a critical one added a day later.
    let w = world().await?;
    let old = w.add_sent("Water the plants").await?;
    w.app.clock.advance(Duration::from_days(1));
    let urgent = w.add("Servers are down", json!({ "need": "critical" })).await?;
    w.send_turn(&urgent).await?;
    let runs_before = w.app.count_rows("item_runs").await?;
    let audit_before = w.app.count_rows("audit_log").await?;

    // Act
    let first = w.next().await?;
    let again = w.next().await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(first.body["item"]["key"], eq(&json!(urgent)));
    expect_that!(first.body["item"]["title"], eq(&json!("Servers are down")));
    expect_that!(first.body["item"]["kind"], eq(&json!("note")));
    expect_that!(first.body["item"]["stage"], eq(&json!("as_written")));
    expect_that!(first.body["item"]["goal"], eq(&json!("with_context")));
    expect_that!(first.body["item"]["reach"], eq(&json!("one")));
    expect_that!(first.body["item"]["runs"], eq(&json!(0)));
    expect_that!(first.body["item"]["failed_runs"], eq(&json!(0)));
    expect_that!(first.body["item"]["heap"]["slug"].is_string(), eq(true));
    expect_that!(first.body["waiting_count"], eq(&json!(0)));
    expect_that!(again.body, eq(&first.body));
    expect_that!(w.app.count_rows("item_runs").await?, eq(runs_before));
    expect_that!(w.app.count_rows("audit_log").await?, eq(audit_before));
    let _ = old;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn only_open_unblocked_items_with_a_turn_and_no_question_are_in_the_queue() -> Result<()> {
    // Arrange: one item for each reason to be left out, and one that is in.
    let w = world().await?;
    let _plain = w.add("No turn", json!({})).await?;
    let blocked = w.add_sent("Blocked").await?;
    w.edit(&blocked, json!({ "state": "blocked" })).await?;
    let asked = w.add_sent("Has a question").await?;
    let question = w
        .app
        .send(
            Method::POST,
            &format!("{}/questions", w.item_path(&asked).await?),
            Some(&w.session),
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    expect_that!(question.status, eq(StatusCode::CREATED));
    let closed = w.add_sent("Closed").await?;
    w.edit(&closed, json!({ "state": "wont_do" })).await?;
    let ready = w.add_sent("Ready").await?;

    // Act
    let next = w.next().await?;

    // Assert
    expect_that!(next.body["item"]["key"], eq(&json!(ready)));
    // Only the item with the open question waits for the person.
    expect_that!(next.body["waiting_count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn next_with_nothing_to_do_says_how_many_wait_for_the_person() -> Result<()> {
    // Arrange: two questions on one item, which count once.
    let w = world().await?;
    let key = w.add_sent("Needs answers").await?;
    for text in ["Which CA?", "Which host?"] {
        let asked = w
            .app
            .send(
                Method::POST,
                &format!("{}/questions", w.item_path(&key).await?),
                Some(&w.session),
                Some(json!({ "kind": "question", "text": text })),
            )
            .await?;
        expect_that!(asked.status, eq(StatusCode::CREATED));
    }

    // Act
    let next = w.next().await?;

    // Assert
    expect_that!(next.status, eq(StatusCode::OK));
    expect_that!(next.body["item"], eq(&Value::Null));
    expect_that!(next.body["waiting_count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_whose_blocker_was_refused_counts_as_waiting_for_the_person() -> Result<()> {
    // Arrange: "Ship the feed" waits for "Pick a host", which is closed as won't do.
    let w = world().await?;
    let blocker = w.add("Pick a host", json!({})).await?;
    let blocked = w.add("Ship the feed", json!({})).await?;
    let blocker_id = w.get(&blocker).await?["id"].clone();
    let edited = w
        .edit(
            &blocked,
            json!({ "state": "blocked", "blocked_by_item_id": blocker_id }),
        )
        .await?;
    anyhow::ensure!(edited.status == StatusCode::OK, "block failed: {}", edited.body);
    let closed = w.edit(&blocker, json!({ "state": "wont_do" })).await?;
    anyhow::ensure!(closed.status == StatusCode::OK, "close failed: {}", closed.body);

    // Act
    let next = w.next().await?;

    // Assert
    expect_that!(next.status, eq(StatusCode::OK));
    expect_that!(next.body["item"], eq(&Value::Null));
    expect_that!(next.body["waiting_count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_heaps_list_is_validated() -> Result<()> {
    // Arrange
    let w = world().await?;
    let other = w.app.register("bob@example.test").await?;
    let other_ref = heap_ref(&w.app, &other.1).await?;
    let archived = w
        .app
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&w.session),
            Some(json!({ "name": "Shared" })),
        )
        .await?;
    let team_slug = archived.body["slug"].as_str().context("slug")?.to_owned();
    let made = w
        .app
        .send(
            Method::POST,
            &format!("/api/v1/teams/{team_slug}/heaps"),
            Some(&w.session),
            Some(json!({ "name": "Old" })),
        )
        .await?;
    let archived_ref = format!("{team_slug}/{}", made.body["slug"].as_str().context("slug")?);
    let archived_id = made.body["id"].as_str().context("id")?.to_owned();
    w.app.set_heap_archived(&archived_id, true).await?;
    let get = |heaps: String| {
        let app = &w.app;
        let session = w.session.clone();
        async move {
            app.send(
                Method::GET,
                &format!("/api/v1/work/next?heaps={heaps}"),
                Some(&session),
                None,
            )
            .await
        }
    };

    // Act
    let empty = get(String::new()).await?;
    let malformed = get("nothing".to_owned()).await?;
    let bad_slug = get("Ada/Personal".to_owned()).await?;
    let too_many = get(vec![w.heap_ref.as_str(); 21].join(",")).await?;
    let twenty_same = get(vec![w.heap_ref.as_str(); 20].join(",")).await?;
    let foreign = get(other_ref).await?;
    let missing = get("ada/nothing".to_owned()).await?;
    let archived = get(archived_ref).await?;
    let claim_empty = w
        .app
        .send(
            Method::POST,
            "/api/v1/work/runs",
            Some(&w.session),
            Some(json!({ "heaps": [] })),
        )
        .await?;

    // Assert
    for (name, response) in [
        ("empty", &empty),
        ("malformed", &malformed),
        ("bad slug", &bad_slug),
        ("too many", &too_many),
        ("claim with none", &claim_empty),
    ] {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY), "{name}");
        expect_that!(code(response), some(eq("invalid_fields")), "{name}");
        expect_that!(response.body["field"], eq(&json!("heaps")), "{name}");
    }
    // A heap named twice counts once, so twenty copies are one heap.
    expect_that!(twenty_same.status, eq(StatusCode::OK));
    expect_that!(foreign.status, eq(StatusCode::NOT_FOUND));
    expect_that!(missing.status, eq(StatusCode::NOT_FOUND));
    expect_that!(archived.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&archived), some(eq("heap_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_claim_starts_a_run_and_the_queue_moves_on() -> Result<()> {
    // Arrange: two items, the first more important.
    let w = world().await?;
    let first = w.add("First", json!({ "need": "critical" })).await?;
    w.send_turn(&first).await?;
    let second = w.add_sent("Second").await?;

    // Act
    let one = w.claim().await?;
    let skipped = w.next().await?;
    let two = w.claim().await?;
    let none = w.claim().await?;

    // Assert
    expect_that!(one.status, eq(StatusCode::CREATED));
    expect_that!(one.body["run"]["item"]["key"], eq(&json!(first)));
    expect_that!(one.body["run"]["end"], eq(&Value::Null));
    expect_that!(one.body["run"]["from_stage"], eq(&json!("as_written")));
    expect_that!(one.body["run"]["item"]["runs"], eq(&json!(1)));
    expect_that!(skipped.body["item"]["key"], eq(&json!(second)));
    expect_that!(two.status, eq(StatusCode::CREATED));
    expect_that!(two.body["run"]["item"]["key"], eq(&json!(second)));
    expect_that!(none.status, eq(StatusCode::OK));
    expect_that!(none.body["run"], eq(&Value::Null));
    // A claim is not audited.
    expect_that!(w.app.count_rows("item_runs").await?, eq(2));
    let item = w.get(&first).await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;
    expect_that!(
        entries.iter().map(|(action, _)| action.as_str()).collect::<Vec<_>>(),
        elements_are![eq(&"item_created"), eq(&"turn_opened")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_golden_turn_takes_a_note_to_with_context() -> Result<()> {
    // Arrange: a heap that prepares every new note.
    let w = world().await?;
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
    expect_that!(set.status, eq(StatusCode::OK));
    let key = w.add("Rotate the certs", json!({})).await?;

    // Act
    let queued = w.next().await?;
    let run = w.claimed().await?;
    let written = w.write_context(&run, &key, "The certs expire on Friday").await?;
    let reached = w.reached(&run).await?;
    let item = w.get(&key).await?;
    let finished = w.finish(&run, json!({ "cost_usd": 0.31 })).await?;
    let again = w.finish(&run, json!({ "cost_usd": 0.31 })).await?;
    let entries = w.app.audit_runs(item["id"].as_str().context("id")?).await?;

    // Assert
    expect_that!(queued.body["item"]["key"], eq(&json!(key)));
    expect_that!(written.status, eq(StatusCode::OK));
    expect_that!(reached.status, eq(StatusCode::OK));
    expect_that!(reached.body["end"], eq(&json!("reached")));
    expect_that!(reached.body["result"], eq(&json!("Added the context")));
    expect_that!(item["stage"], eq(&json!("with_context")));
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(item["with_agent"], eq(false));
    expect_that!(item["next_goal"], eq(&json!("defined")));
    expect_that!(actions(&item), contains(eq(&"send_to_agent")));
    expect_that!(finished.status, eq(StatusCode::OK));
    expect_that!(finished.body["end"], eq(&json!("reached")));
    expect_that!(finished.body["cost_usd"], eq(&json!(0.31)));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&again), some(eq("run_cost_recorded")));
    expect_that!(again.body["title"], eq(&json!("The run's cost is already recorded.")));
    // The write and the turn's end are audited with the run; the creation and the turn's opening are not.
    let in_run: Vec<(&str, bool)> = entries
        .iter()
        .map(|(action, run_id)| (action.as_str(), run_id.as_deref() == Some(run.as_str())))
        .collect();
    expect_that!(
        in_run,
        elements_are![
            eq(&("item_created", false)),
            eq(&("turn_opened", false)),
            eq(&("item_updated", true)),
            eq(&("turn_reached", true)),
        ]
    );
    // The stage change is bundled with the end of the turn.
    let reached_diff = w
        .app
        .audit_entries(item["id"].as_str().context("id")?)
        .await?
        .into_iter()
        .find(|(action, _)| action == "turn_reached")
        .context("no turn_reached entry")?
        .1;
    expect_that!(reached_diff["goal"], eq(&json!("with_context")));
    expect_that!(reached_diff["stage"]["before"], eq(&json!("as_written")));
    expect_that!(reached_diff["stage"]["after"], eq(&json!("with_context")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_sent_all_the_way_opens_the_next_turn_when_reached() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add("Rotate the certs", json!({})).await?;
    w.send_all_the_way(&key).await?;
    let run = w.claimed().await?;
    let written = w.write_context(&run, &key, "The certs expire on Friday").await?;

    // Act
    let reached = w.reached(&run).await?;
    let item = w.get(&key).await?;
    let queued = w.next().await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;
    let runs = w.app.audit_runs(item["id"].as_str().context("id")?).await?;
    let finished = w.finish(&run, json!({ "cost_usd": 0.31 })).await?;

    // Assert
    expect_that!(written.status, eq(StatusCode::OK));
    expect_that!(reached.status, eq(StatusCode::OK));
    expect_that!(reached.body["end"], eq(&json!("reached")));
    // The run still describes the turn it belonged to.
    expect_that!(reached.body["item"]["goal"], eq(&json!("with_context")));
    expect_that!(item["stage"], eq(&json!("with_context")));
    expect_that!(item["with_agent"], eq(true));
    expect_that!(item["turn"]["goal"], eq(&json!("defined")));
    expect_that!(item["turn"]["reach"], eq(&json!("all_the_way")));
    expect_that!(item["turn"]["line"], eq(&json!("Use the new CA.")));
    expect_that!(item["turn"]["ended"], eq(&Value::Null));
    expect_that!(queued.body["item"]["key"], eq(&json!(key)));
    expect_that!(finished.status, eq(StatusCode::OK));
    expect_that!(finished.body["end"], eq(&json!("reached")));
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![
            eq(&"item_created"),
            eq(&"turn_opened"),
            eq(&"item_updated"),
            eq(&"turn_reached"),
            eq(&"turn_opened"),
        ]
    );
    expect_that!(runs[3].1.as_deref(), some(eq(run.as_str())));
    expect_that!(runs[4].1.as_deref(), some(eq(run.as_str())));
    let opened = &entries[4].1;
    expect_that!(opened["reach"], eq(&json!("all_the_way")));
    expect_that!(opened["goal"], eq(&json!("defined")));
    expect_that!(opened["line"], eq(&json!("Use the new CA.")));
    expect_that!(opened["source"], eq(&json!("go_ahead")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_sent_all_the_way_goes_on_to_defined_and_then_to_done() -> Result<()> {
    // Arrange: a note with context, sent all the way, whose first turn goes to defined.
    let w = world().await?;
    let key = w
        .add("Rotate the certs", json!({ "description": "Why and how" }))
        .await?;
    w.edit(&key, json!({ "stage": "with_context" })).await?;
    w.send_all_the_way(&key).await?;
    let run = w.claimed().await?;
    let version = w.get(&key).await?["version"].clone();
    let made_task = w
        .in_run(
            &run,
            Method::PATCH,
            &w.item_path(&key).await?,
            Some(&version),
            Some(json!({ "kind": "task" })),
        )
        .await?;
    let step = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/steps", w.item_path(&key).await?),
            None,
            Some(json!({ "title": "Order the certs" })),
        )
        .await?;

    // Act
    let reached = w.reached(&run).await?;
    let item = w.get(&key).await?;
    let second = w.claimed().await?;
    let too_early = w.reached(&second).await?;
    let still = w.get(&key).await?;

    // Assert
    expect_that!(made_task.status, eq(StatusCode::OK));
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(reached.status, eq(StatusCode::OK));
    expect_that!(item["stage"], eq(&json!("defined")));
    expect_that!(item["turn"]["goal"], eq(&json!("done")));
    expect_that!(item["turn"]["reach"], eq(&json!("all_the_way")));
    expect_that!(item["turn"]["line"], eq(&json!("Use the new CA.")));
    // Done never passes yet, and the turn stays open.
    expect_that!(too_early.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(still["turn"]["goal"], eq(&json!("done")));
    expect_that!(still["with_agent"], eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_of_one_state_leaves_the_item_to_the_person_when_reached() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let _ = w.write_context(&run, &key, "Why and how").await?;

    // Act
    let reached = w.reached(&run).await?;
    let item = w.get(&key).await?;

    // Assert
    expect_that!(reached.status, eq(StatusCode::OK));
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(item["with_agent"], eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_next_turn_goes_where_the_item_is_after_the_person_went_past_the_goal() -> Result<()> {
    // Arrange: a note sent all the way, which the person makes a defined task while the agent works.
    let w = world().await?;
    let key = w.add("Rotate the certs", json!({})).await?;
    w.send_all_the_way(&key).await?;
    let run = w.claimed().await?;
    let described = w
        .edit(
            &key,
            json!({ "description": "Rotate them before Friday", "kind": "task" }),
        )
        .await?;
    let still_open = w.get(&key).await?;

    // Act
    let defined = w.edit(&key, json!({ "stage": "defined" })).await?;
    let reached = w.reached(&run).await?;
    let item = w.get(&key).await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;

    // Assert: the edit ended the turn that went to with context, and the one to done opened from the item as it is.
    expect_that!(described.status, eq(StatusCode::OK));
    expect_that!(still_open["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(defined.status, eq(StatusCode::OK));
    expect_that!(reached.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&reached), some(eq("run_over")));
    expect_that!(item["stage"], eq(&json!("defined")));
    expect_that!(item["turn"]["goal"], eq(&json!("done")));
    expect_that!(item["turn"]["reach"], eq(&json!("all_the_way")));
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![
            eq(&"item_created"),
            eq(&"turn_opened"),
            eq(&"item_updated"),
            eq(&"item_updated"),
            eq(&"turn_reached"),
            eq(&"turn_opened"),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_miss_names_the_first_condition_and_leaves_the_run_live() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;

    // Act
    let no_context = w.reached(&run).await?;
    let still_live = w.next().await?;
    let _ = w.write_context(&run, &key, "Why and how").await?;
    let asked = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/questions", w.item_path(&key).await?),
            None,
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    let open_question = w.reached(&run).await?;

    // Assert
    expect_that!(no_context.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&no_context), some(eq("not_reached")));
    expect_that!(no_context.body["title"], eq(&json!("The item has no context yet.")));
    // The run is still live, so the item isn't offered again.
    expect_that!(still_live.body["item"], eq(&Value::Null));
    expect_that!(asked.status, eq(StatusCode::CREATED));
    expect_that!(open_question.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(
        open_question.body["title"],
        eq(&json!("The item has an open question."))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn defined_needs_a_task_with_context_and_a_step() -> Result<()> {
    // Arrange: a note with context, whose turn goes to defined.
    let w = world().await?;
    let key = w
        .add("Rotate the certs", json!({ "description": "Why and how" }))
        .await?;
    w.edit(&key, json!({ "stage": "with_context" })).await?;
    w.send_turn(&key).await?;
    let run = w.claimed().await?;

    // Act
    let note = w.reached(&run).await?;
    let version = w.get(&key).await?["version"].clone();
    let made_task = w
        .in_run(
            &run,
            Method::PATCH,
            &w.item_path(&key).await?,
            Some(&version),
            Some(json!({ "kind": "task" })),
        )
        .await?;
    let no_steps = w.reached(&run).await?;
    let step = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/steps", w.item_path(&key).await?),
            None,
            Some(json!({ "title": "Order the certs" })),
        )
        .await?;
    let reached = w.reached(&run).await?;
    let item = w.get(&key).await?;

    // Assert
    expect_that!(
        note.body["title"],
        eq(&json!("The item is a note; a defined item is a task."))
    );
    expect_that!(made_task.status, eq(StatusCode::OK));
    expect_that!(no_steps.body["title"], eq(&json!("The task has no steps.")));
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(reached.status, eq(StatusCode::OK));
    expect_that!(item["stage"], eq(&json!("defined")));
    expect_that!(item["kind"], eq(&json!("task")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_agent_neither_reaches_done_nor_closes_a_task() -> Result<()> {
    // Arrange: a defined task, whose turn goes to done.
    let w = world().await?;
    let key = w
        .add("Rotate the certs", json!({ "description": "Why and how" }))
        .await?;
    w.edit(&key, json!({ "kind": "task", "stage": "defined" })).await?;
    w.send_turn(&key).await?;
    let run = w.claimed().await?;

    // Act
    let reached = w.reached(&run).await?;
    let done = w
        .in_run(
            &run,
            Method::PATCH,
            &w.item_path(&key).await?,
            Some(&w.get(&key).await?["version"]),
            Some(json!({ "state": "done" })),
        )
        .await?;

    // Assert
    expect_that!(reached.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(
        reached.body["title"],
        eq(&json!(
            "A task reaches done when the person agrees to close it. Propose it with `upheap question propose-done`."
        ))
    );
    expect_that!(done.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&done), some(eq("run_done_needs_review")));
    expect_that!(
        done.body["title"],
        eq(&json!(
            "An agent never closes a task. When nothing is left, propose closing with `upheap question propose-done`."
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reached_bounds_the_result_and_the_links() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    w.write_context(&run, &key, "Why and how").await?;

    // Act
    let blank = w.run_call(&run, "reached", Some(json!({ "result": "  " }))).await?;
    let long = w
        .run_call(&run, "reached", Some(json!({ "result": "x".repeat(2_001) })))
        .await?;
    let relative = w
        .run_call(
            &run,
            "reached",
            Some(json!({ "result": "ok", "links": ["example.com"] })),
        )
        .await?;
    let many = w
        .run_call(
            &run,
            "reached",
            Some(json!({ "result": "ok", "links": vec!["https://example.com"; 11] })),
        )
        .await?;
    let unknown = w
        .run_call(&run, "reached", Some(json!({ "result": "ok", "extra": 1 })))
        .await?;
    let fine = w
        .run_call(
            &run,
            "reached",
            Some(json!({ "result": " Done ", "links": ["https://example.com/pr/1"] })),
        )
        .await?;

    // Assert
    for response in [&blank, &long, &relative, &many] {
        expect_that!(
            response.status,
            eq(StatusCode::UNPROCESSABLE_ENTITY),
            "{}",
            response.body
        );
    }
    expect_that!(unknown.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(fine.status, eq(StatusCode::OK));
    expect_that!(fine.body["result"], eq(&json!("Done")));
    expect_that!(fine.body["links"], eq(&json!(["https://example.com/pr/1"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_may_write_the_context_steps_questions_and_start_but_nothing_else() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let path = w.item_path(&key).await?;
    let version = || async { Ok::<_, anyhow::Error>(w.get(&key).await?["version"].clone()) };

    // Act
    let context = w.write_context(&run, &key, "Why and how").await?;
    let to_task = w
        .in_run(
            &run,
            Method::PATCH,
            &path,
            Some(&version().await?),
            Some(json!({ "kind": "task" })),
        )
        .await?;
    let step = w
        .in_run(
            &run,
            Method::POST,
            &format!("{path}/steps"),
            None,
            Some(json!({ "title": "Order the certs" })),
        )
        .await?;
    let step_version = step.body["steps"][0]["version"].clone();
    let ticked = w
        .in_run(
            &run,
            Method::PATCH,
            &format!("{path}/steps/1"),
            Some(&step_version),
            Some(json!({ "state": "ticked" })),
        )
        .await?;
    let dropped = w
        .in_run(
            &run,
            Method::PATCH,
            &format!("{path}/steps/1"),
            Some(&ticked.body["steps"][0]["version"]),
            Some(json!({ "state": "dropped" })),
        )
        .await?;
    let asked = w
        .in_run(
            &run,
            Method::POST,
            &format!("{path}/questions"),
            None,
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    let started = w
        .in_run(
            &run,
            Method::PATCH,
            &path,
            Some(&version().await?),
            Some(json!({ "state": "in_progress" })),
        )
        .await?;

    // Assert
    expect_that!(context.status, eq(StatusCode::OK));
    expect_that!(to_task.status, eq(StatusCode::OK));
    expect_that!(to_task.body["kind"], eq(&json!("task")));
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(ticked.status, eq(StatusCode::OK));
    expect_that!(dropped.status, eq(StatusCode::OK));
    expect_that!(asked.status, eq(StatusCode::CREATED));
    expect_that!(started.status, eq(StatusCode::OK));
    expect_that!(started.body["state"], eq(&json!("in_progress")));
    // Starting in a run keeps the turn: an agent sets a task in progress when it starts doing it.
    expect_that!(started.body["turn"]["goal"], eq(&json!("with_context")));
    // The question remembers its run.
    let stored: i64 = w.app.count_rows("item_questions WHERE run_id IS NOT NULL").await?;
    expect_that!(stored, eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn every_other_write_in_a_run_is_refused() -> Result<()> {
    // Arrange: a question for the person to reply to, a task to send, and a run on another item.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let path = w.item_path(&key).await?;
    let run = w.claimed().await?;
    let asked = w
        .in_run(
            &run,
            Method::POST,
            &format!("{path}/questions"),
            None,
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    let question_version = asked.body["questions"][0]["version"].clone();
    let version = w.get(&key).await?["version"].clone();
    let heap_path = w.app.heap_url(&w.heap).await?;
    let refused = |name: &'static str, response: TestResponse| {
        expect_that!(response.status, eq(StatusCode::FORBIDDEN), "{name}: {}", response.body);
        expect_that!(code(&response), some(eq("run_forbidden")), "{name}");
        expect_that!(
            response.body["title"],
            eq(&json!("Only the person does that.")),
            "{name}"
        );
    };

    // Act and assert
    let patches = [
        ("title", json!({ "title": "Other" })),
        ("need", json!({ "need": "critical" })),
        ("stage", json!({ "stage": "with_context" })),
        ("tags", json!({ "tag_ids": [] })),
        ("wont do", json!({ "state": "wont_do" })),
        ("blocked", json!({ "state": "blocked" })),
        ("a mix", json!({ "description": "ok", "title": "Other" })),
    ];
    for (name, body) in patches {
        let response = w.in_run(&run, Method::PATCH, &path, Some(&version), Some(body)).await?;
        refused(name, response);
    }
    refused(
        "reply",
        w.in_run(
            &run,
            Method::PATCH,
            &format!("{path}/questions/1"),
            Some(&question_version),
            Some(json!({ "reply": { "comment": "Internal" } })),
        )
        .await?,
    );
    refused(
        "send",
        w.in_run(
            &run,
            Method::POST,
            &format!("{path}/turn"),
            None,
            Some(json!({ "reach": "one" })),
        )
        .await?,
    );
    refused(
        "take back",
        w.in_run(&run, Method::DELETE, &format!("{path}/turn"), None, None)
            .await?,
    );
    refused(
        "retry",
        w.in_run(&run, Method::POST, &format!("{path}/turn/retry"), None, None)
            .await?,
    );
    refused(
        "new item",
        w.in_run(
            &run,
            Method::POST,
            &format!("{heap_path}/items"),
            None,
            Some(json!({ "title": "New", "need": "expected" })),
        )
        .await?,
    );
    let heap = w.app.send(Method::GET, &heap_path, Some(&w.session), None).await?;
    refused(
        "heap patch",
        w.in_run(
            &run,
            Method::PATCH,
            &heap_path,
            Some(&heap.body["version"]),
            Some(json!({ "name": "Renamed" })),
        )
        .await?,
    );
    refused(
        "claim",
        w.in_run(
            &run,
            Method::POST,
            "/api/v1/work/runs",
            None,
            Some(json!({ "heaps": [w.heap_ref] })),
        )
        .await?,
    );
    // Reading is always allowed.
    let read = w.in_run(&run, Method::GET, &path, None, None).await?;
    expect_that!(read.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_writes_only_to_its_own_item_and_only_while_it_lasts() -> Result<()> {
    // Arrange: two items; the run holds the more important one.
    let w = world().await?;
    let mine = w.add("Mine", json!({ "need": "critical" })).await?;
    w.send_turn(&mine).await?;
    let other = w.add("Other", json!({})).await?;
    let run = w.claimed().await?;

    // Act
    let wrong_item = w.write_context(&run, &other, "Not mine").await?;
    let unknown_run = w
        .write_context("018f0000-0000-7000-8000-000000000000", &mine, "Nobody's")
        .await?;
    w.write_context(&run, &mine, "Why and how").await?;
    let reached = w.reached(&run).await?;
    let after = w.write_context(&run, &mine, "Too late").await?;
    let heartbeat = w.run_call(&run, "heartbeat", None).await?;
    let reached_again = w.reached(&run).await?;

    // Assert
    expect_that!(wrong_item.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&wrong_item), some(eq("run_wrong_item")));
    expect_that!(
        wrong_item.body["title"],
        eq(&json!(format!("This run is for item {mine}.")))
    );
    expect_that!(wrong_item.body["item_key"], eq(&json!(mine)));
    expect_that!(unknown_run.status, eq(StatusCode::NOT_FOUND));
    expect_that!(reached.status, eq(StatusCode::OK));
    for response in [&after, &heartbeat, &reached_again] {
        expect_that!(response.status, eq(StatusCode::CONFLICT));
        expect_that!(code(response), some(eq("run_over")));
        expect_that!(response.body["title"], eq(&json!("This run is over.")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn taking_the_item_back_mid_run_ends_the_run_and_withdraws_the_agents_questions() -> Result<()> {
    // Arrange: the agent ticks a step and asks a question; the person has a question of their own.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let path = w.item_path(&key).await?;
    let own = w
        .app
        .send(
            Method::POST,
            &format!("{path}/questions"),
            Some(&w.session),
            Some(json!({ "kind": "question", "text": "My own question" })),
        )
        .await?;
    expect_that!(own.status, eq(StatusCode::CREATED));
    // The person's question holds the item, so answer it first.
    let version = own.body["questions"][0]["version"].clone();
    let answered = w
        .app
        .patch(
            &w.session,
            &format!("{path}/questions/1"),
            &version,
            json!({ "reply": { "comment": "Fine" } }),
        )
        .await?;
    expect_that!(answered.status, eq(StatusCode::OK));
    let own2 = w
        .app
        .send(
            Method::POST,
            &format!("{path}/questions"),
            Some(&w.session),
            Some(json!({ "kind": "question", "text": "Still open" })),
        )
        .await?;
    expect_that!(own2.status, eq(StatusCode::CREATED));
    w.edit(&key, json!({ "kind": "task" })).await?;
    let ready = w.claim().await?;
    // The item has an open question of the person's own, so it isn't in the queue; answer to get it claimed.
    expect_that!(ready.body["run"], eq(&Value::Null));
    let version = w.get(&key).await?["questions"][1]["version"].clone();
    w.app
        .patch(
            &w.session,
            &format!("{path}/questions/2"),
            &version,
            json!({ "reply": { "comment": "Answered" } }),
        )
        .await?;
    let run = w.claimed().await?;
    let step = w
        .in_run(
            &run,
            Method::POST,
            &format!("{path}/steps"),
            None,
            Some(json!({ "title": "Order the certs" })),
        )
        .await?;
    w.in_run(
        &run,
        Method::PATCH,
        &format!("{path}/steps/1"),
        Some(&step.body["steps"][0]["version"]),
        Some(json!({ "state": "ticked" })),
    )
    .await?;
    let agent = w
        .in_run(
            &run,
            Method::POST,
            &format!("{path}/questions"),
            None,
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    expect_that!(agent.status, eq(StatusCode::CREATED));
    let person = w
        .app
        .send(
            Method::POST,
            &format!("{path}/questions"),
            Some(&w.session),
            Some(json!({ "kind": "question", "text": "A question from the person" })),
        )
        .await?;
    expect_that!(person.status, eq(StatusCode::CREATED));

    // Act
    let taken = w
        .app
        .send(Method::DELETE, &format!("{path}/turn"), Some(&w.session), None)
        .await?;
    let next_write = w.write_context(&run, &key, "Too late").await?;
    let finished = w.finish(&run, json!({})).await?;

    // Assert
    expect_that!(taken.status, eq(StatusCode::OK));
    let item = &taken.body;
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(item["steps"][0]["state"], eq(&json!("ticked")));
    let states: Vec<(&str, &str)> = item["questions"]
        .as_array()
        .context("questions")?
        .iter()
        .map(|q| {
            (
                q["text"].as_str().unwrap_or_default(),
                q["state"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    expect_that!(states, contains(eq(&("Which CA?", "withdrawn"))));
    expect_that!(states, contains(eq(&("A question from the person", "open"))));
    expect_that!(next_write.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&next_write), some(eq("run_over")));
    // finish records the cost only; the run keeps its end.
    expect_that!(finished.status, eq(StatusCode::OK));
    expect_that!(finished.body["end"], eq(&json!("taken_back")));
    // Taking back is no failure: nothing counts toward stuck.
    let withdrawn_entries = w
        .app
        .audit_entries(item["id"].as_str().context("id")?)
        .await?
        .into_iter()
        .filter(|(action, _)| action == "question_withdrawn")
        .count();
    expect_that!(withdrawn_entries, eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_version_in_a_run_conflicts_and_passes_after_a_fresh_read() -> Result<()> {
    // Arrange: the person renames the item while the agent is about to write the context.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let stale = w.get(&key).await?["version"].clone();
    w.edit(&key, json!({ "title": "Rotate all the certs" })).await?;
    let path = w.item_path(&key).await?;

    // Act
    let conflict = w
        .in_run(
            &run,
            Method::PATCH,
            &path,
            Some(&stale),
            Some(json!({ "description": "Why and how" })),
        )
        .await?;
    let fresh = w.write_context(&run, &key, "Why and how").await?;

    // Assert
    expect_that!(conflict.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&conflict), some(eq("version_conflict")));
    expect_that!(fresh.status, eq(StatusCode::OK));
    expect_that!(fresh.body["description"], eq(&json!("Why and how")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_that_asked_ends_as_asked_and_forgives_earlier_failures() -> Result<()> {
    // Arrange: one failed run, then a run that asks.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    w.fail_once().await?;
    let run = w.claimed().await?;
    let asked = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/questions", w.item_path(&key).await?),
            None,
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    expect_that!(asked.status, eq(StatusCode::CREATED));

    // Act
    let finished = w.finish(&run, json!({})).await?;
    let item = w.get(&key).await?;

    // Assert
    expect_that!(finished.status, eq(StatusCode::OK));
    expect_that!(finished.body["end"], eq(&json!("asked")));
    expect_that!(finished.body["failure"], eq(&Value::Null));
    expect_that!(item["turn"]["failed_runs"], eq(&json!(0)));
    expect_that!(item["turn"]["runs"], eq(&json!(2)));
    expect_that!(item["waits_for_you"], eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_that_ends_without_a_word_fails_with_a_default_or_the_given_reason() -> Result<()> {
    // Arrange
    let w = world().await?;
    let _key = w.add_sent("Rotate the certs").await?;
    let first = w.claimed().await?;

    // Act
    let plain = w.finish(&first, json!({})).await?;
    let second = w.claimed().await?;
    let long = w
        .finish(&second, json!({ "failure": format!("  {}  ", "é".repeat(2_500)) }))
        .await?;
    let third = w.claimed().await?;
    let bad_cost = w.finish(&third, json!({ "cost_usd": -1 })).await?;

    // Assert
    expect_that!(plain.status, eq(StatusCode::OK));
    expect_that!(plain.body["end"], eq(&json!("failed")));
    expect_that!(
        plain.body["failure"],
        eq(&json!(
            "The run ended without saying it reached with context or asking a question."
        ))
    );
    expect_that!(
        long.body["failure"].as_str().map(|text| text.chars().count()),
        some(eq(2_000))
    );
    expect_that!(bad_cost.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(code(&bad_cost), some(eq("invalid_fields")));
    expect_that!(bad_cost.body["field"], eq(&json!("cost_usd")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heartbeat_keeps_a_run_live_past_the_lapse() -> Result<()> {
    // Arrange
    let w = world().await?;
    let _key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;

    // Act
    w.app.clock.advance(Duration::from_minutes(20));
    let beat = w.run_call(&run, "heartbeat", None).await?;
    w.app.clock.advance(Duration::from_minutes(20));
    let later = w.next().await?;
    let second_beat = w.run_call(&run, "heartbeat", None).await?;
    let finished = w.finish(&run, json!({})).await?;
    let over = w.run_call(&run, "heartbeat", None).await?;

    // Assert
    expect_that!(beat.status, eq(StatusCode::NO_CONTENT));
    // 40 minutes after the claim, but only 20 after the heartbeat.
    expect_that!(later.body["item"], eq(&Value::Null));
    expect_that!(second_beat.status, eq(StatusCode::NO_CONTENT));
    expect_that!(finished.body["end"], eq(&json!("failed")));
    expect_that!(over.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&over), some(eq("run_over")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_write_in_a_run_counts_as_hearing_from_it() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;

    // Act
    w.app.clock.advance(Duration::from_minutes(25));
    let write = w.write_context(&run, &key, "Why and how").await?;
    w.app.clock.advance(Duration::from_minutes(25));
    let still = w.next().await?;

    // Assert
    expect_that!(write.status, eq(StatusCode::OK));
    expect_that!(still.body["item"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn going_past_the_goal_by_hand_ends_the_turn_and_a_late_reached_is_run_over() -> Result<()> {
    // Arrange: a note sent for context, whose person makes it a defined task while the agent works.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let described = w
        .edit(
            &key,
            json!({ "description": "Rotate them before Friday", "kind": "task" }),
        )
        .await?;

    // Act
    let defined = w.edit(&key, json!({ "stage": "defined" })).await?;
    let reached = w.reached(&run).await?;
    let finished = w.finish(&run, json!({ "cost_usd": 0.1 })).await?;
    let item = w.get(&key).await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;

    // Assert
    expect_that!(described.status, eq(StatusCode::OK));
    expect_that!(defined.status, eq(StatusCode::OK));
    expect_that!(reached.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&reached), some(eq("run_over")));
    expect_that!(finished.body["end"], eq(&json!("taken_back")));
    expect_that!(item["stage"], eq(&json!("defined")));
    expect_that!(item["turn"], eq(&Value::Null));
    let reached_diff = &entries
        .iter()
        .find(|(action, _)| action == "turn_reached")
        .context("no turn_reached entry")?
        .1;
    expect_that!(reached_diff["goal"], eq(&json!("with_context")));
    expect_that!(reached_diff["stage"]["before"], eq(&json!("as_written")));
    expect_that!(reached_diff["stage"]["after"], eq(&json!("defined")));
    expect_that!(reached_diff["how"], eq(&json!("stage_set")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_set_to_defined_by_hand_after_agents_prepared_it_costs_no_run() -> Result<()> {
    // Arrange: a heap that prepares every new item; a task is added, so a turn to defined opens.
    let w = world().await?;
    let path = w.app.heap_url(&w.heap).await?;
    let heap = w.app.send(Method::GET, &path, Some(&w.session), None).await?;
    w.app
        .patch(
            &w.session,
            &path,
            &heap.body["version"],
            json!({ "agents_prepare": true }),
        )
        .await?;
    let key = w
        .add(
            "Rotate the certs",
            json!({ "kind": "task", "description": "Why and how" }),
        )
        .await?;
    let opened = w.get(&key).await?;
    let step = w
        .app
        .send(
            Method::POST,
            &format!("{}/steps", w.item_path(&key).await?),
            Some(&w.session),
            Some(json!({ "title": "Order the certs" })),
        )
        .await?;

    // Act
    let defined = w.edit(&key, json!({ "stage": "defined" })).await?;
    let item = w.get(&key).await?;
    let claim = w.claim().await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;
    let runs = w.app.audit_runs(item["id"].as_str().context("id")?).await?;
    let history = w
        .app
        .send(
            Method::GET,
            &format!(
                "{}/audit?entity_id={}",
                w.app.heap_url(&w.heap).await?,
                item["id"].as_str().context("id")?
            ),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(opened["turn"]["goal"], eq(&json!("defined")));
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(defined.status, eq(StatusCode::OK));
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(item["with_agent"], eq(false));
    expect_that!(claim.status, eq(StatusCode::OK));
    expect_that!(claim.body["run"], eq(&Value::Null));
    expect_that!(w.app.count_rows("item_runs").await?, eq(0));
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![
            eq(&"item_created"),
            eq(&"turn_opened"),
            eq(&"step_added"),
            eq(&"item_updated"),
            eq(&"turn_reached"),
        ]
    );
    let diff = &entries[4].1;
    expect_that!(diff["goal"], eq(&json!("defined")));
    expect_that!(
        diff["stage"],
        eq(&json!({ "before": "as_written", "after": "defined" }))
    );
    expect_that!(diff["how"], eq(&json!("stage_set")));
    // The person did it, outside any run.
    expect_that!(runs[4].1, none());
    let latest = &history.body["entries"][0];
    expect_that!(latest["phrase"], eq(&json!("reached the goal of a turn on")));
    expect_that!(
        latest["changes"],
        eq(&json!([
            { "field": "goal", "label": "Goal", "before": null, "after": "Defined" },
            { "field": "how", "label": "How", "before": null, "after": "Stage set by hand" },
            { "field": "stage", "label": "Stage", "before": "As written", "after": "Defined" },
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_sent_all_the_way_to_defined_opens_the_one_to_done_when_set_by_hand() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add("Rotate the certs", json!({ "kind": "task" })).await?;
    w.send_all_the_way(&key).await?;

    // Act
    let defined = w.edit(&key, json!({ "stage": "defined" })).await?;
    let item = w.get(&key).await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;
    let claim = w.claim().await?;

    // Assert
    expect_that!(defined.status, eq(StatusCode::OK));
    expect_that!(item["turn"]["goal"], eq(&json!("done")));
    expect_that!(item["turn"]["line"], eq(&json!("Use the new CA.")));
    expect_that!(item["with_agent"], eq(true));
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(
        actions,
        elements_are![
            eq(&"item_created"),
            eq(&"turn_opened"),
            eq(&"item_updated"),
            eq(&"turn_reached"),
            eq(&"turn_opened"),
        ]
    );
    expect_that!(claim.status, eq(StatusCode::CREATED));
    expect_that!(claim.body["run"]["item"]["goal"], eq(&json!("done")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_the_stage_to_the_goal_mid_run_takes_the_run_back_and_withdraws_its_questions() -> Result<()> {
    // Arrange: the agent asks a question; the person has an open question of their own.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let path = w.item_path(&key).await?;
    let run = w.claimed().await?;
    let agent = w
        .in_run(
            &run,
            Method::POST,
            &format!("{path}/questions"),
            None,
            Some(json!({ "kind": "question", "text": "Which CA?" })),
        )
        .await?;
    let person = w
        .app
        .send(
            Method::POST,
            &format!("{path}/questions"),
            Some(&w.session),
            Some(json!({ "kind": "question", "text": "My own question" })),
        )
        .await?;

    // Act
    let set = w.edit(&key, json!({ "stage": "with_context" })).await?;
    let reached = w.reached(&run).await?;
    let finished = w.finish(&run, json!({})).await?;
    let item = w.get(&key).await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;

    // Assert
    expect_that!(agent.status, eq(StatusCode::CREATED));
    expect_that!(person.status, eq(StatusCode::CREATED));
    expect_that!(set.status, eq(StatusCode::OK));
    expect_that!(reached.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&reached), some(eq("run_over")));
    expect_that!(finished.body["end"], eq(&json!("taken_back")));
    expect_that!(item["turn"], eq(&Value::Null));
    let states: Vec<(&str, &str)> = item["questions"]
        .as_array()
        .context("questions")?
        .iter()
        .map(|q| {
            (
                q["text"].as_str().unwrap_or_default(),
                q["state"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    expect_that!(states, contains(eq(&("Which CA?", "withdrawn"))));
    expect_that!(states, contains(eq(&("My own question", "open"))));
    let withdrawn: Vec<&Value> = entries
        .iter()
        .filter(|(action, _)| action == "question_withdrawn")
        .map(|(_, diff)| diff)
        .collect();
    expect_that!(withdrawn.len(), eq(1));
    expect_that!(withdrawn[0]["why"], eq(&json!("goal_met")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_edit_that_does_not_reach_the_goal_leaves_the_turn_open() -> Result<()> {
    // Arrange: a note going to with context, and a task going to defined.
    let w = world().await?;
    let note = w.add_sent("A note").await?;
    let task = w.add("A task", json!({ "kind": "task" })).await?;
    w.send_turn(&task).await?;

    // Act
    let retitled = w.edit(&note, json!({ "description": "More about it" })).await?;
    let short = w.edit(&task, json!({ "stage": "with_context" })).await?;

    // Assert
    expect_that!(retitled.status, eq(StatusCode::OK));
    expect_that!(short.status, eq(StatusCode::OK));
    expect_that!(w.get(&note).await?["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(w.get(&task).await?["turn"]["goal"], eq(&json!("defined")));
    expect_that!(w.get(&task).await?["with_agent"], eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_claim_ends_a_turn_already_at_its_goal_and_takes_the_next_item() -> Result<()> {
    // Arrange: two sent notes, the first more important and put at its goal behind the edit's back.
    let w = world().await?;
    let met = w.add("Met", json!({ "need": "critical" })).await?;
    w.send_turn(&met).await?;
    let other = w.add_sent("Other").await?;
    let met_id = w.get(&met).await?["id"].as_str().context("id")?.to_owned();
    w.app.set_item_stage(&met_id, "with_context").await?;

    // Act
    let offered = w.next().await?;
    let claimed = w.claim().await?;
    let entries = w.app.audit_entries(&met_id).await?;
    let item = w.get(&met).await?;

    // Assert
    expect_that!(offered.body["item"]["key"], eq(&json!(other)));
    expect_that!(claimed.status, eq(StatusCode::CREATED));
    expect_that!(claimed.body["run"]["item"]["key"], eq(&json!(other)));
    expect_that!(w.app.count_rows("item_runs").await?, eq(1));
    expect_that!(item["turn"], eq(&Value::Null));
    let reached = entries
        .iter()
        .find(|(action, _)| action == "turn_reached")
        .context("no turn_reached entry")?;
    expect_that!(reached.1["how"], eq(&json!("already_met")));
    // The stage didn't change, so the entry doesn't say it did.
    expect_that!(reached.1.get("stage"), none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_claim_with_only_a_met_turn_ends_it_and_starts_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Met").await?;
    let id = w.get(&key).await?["id"].as_str().context("id")?.to_owned();
    w.app.set_item_stage(&id, "with_context").await?;

    // Act
    let offered = w.next().await?;
    let entries_after_next = w.app.audit_entries(&id).await?.len();
    let claimed = w.claim().await?;

    // Assert
    expect_that!(offered.body["item"], eq(&Value::Null));
    // `next` writes nothing: the turn is still open until a claim ends it.
    expect_that!(entries_after_next, eq(2));
    expect_that!(claimed.status, eq(StatusCode::OK));
    expect_that!(claimed.body["run"], eq(&Value::Null));
    expect_that!(w.app.count_rows("item_runs").await?, eq(0));
    expect_that!(w.get(&key).await?["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_claim_starts_a_run_on_the_follow_on_of_a_met_turn() -> Result<()> {
    // Arrange: a task sent all the way to defined, put at defined behind the edit's back.
    let w = world().await?;
    let key = w.add("Rotate the certs", json!({ "kind": "task" })).await?;
    w.send_all_the_way(&key).await?;
    let id = w.get(&key).await?["id"].as_str().context("id")?.to_owned();
    w.app.set_item_stage(&id, "defined").await?;

    // Act
    let claimed = w.claim().await?;

    // Assert
    expect_that!(claimed.status, eq(StatusCode::CREATED));
    expect_that!(claimed.body["run"]["item"]["key"], eq(&json!(key)));
    expect_that!(claimed.body["run"]["item"]["goal"], eq(&json!("done")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_hands_the_turn_back_and_it_is_no_failure() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let id = w.get(&key).await?["id"].as_str().context("id")?.to_owned();

    // Act
    let handed = w
        .run_call(
            &run,
            "hand-back",
            Some(json!({ "reason": "  Already has its context  " })),
        )
        .await?;
    let item = w.get(&key).await?;
    let again = w
        .run_call(&run, "hand-back", Some(json!({ "reason": "Again" })))
        .await?;
    let reached = w.reached(&run).await?;
    let finished = w.finish(&run, json!({ "cost_usd": 0.2 })).await?;
    let entries = w.app.audit_entries(&id).await?;
    let runs = w.app.audit_runs(&id).await?;
    let history = w
        .app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={id}", w.app.heap_url(&w.heap).await?),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(handed.status, eq(StatusCode::OK));
    expect_that!(handed.body["end"], eq(&json!("handed_back")));
    expect_that!(handed.body["result"], eq(&json!("Already has its context")));
    expect_that!(handed.body["failure"], eq(&Value::Null));
    expect_that!(item["stage"], eq(&json!("as_written")));
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(item["with_agent"], eq(false));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&again), some(eq("run_over")));
    expect_that!(reached.status, eq(StatusCode::CONFLICT));
    expect_that!(finished.status, eq(StatusCode::OK));
    expect_that!(finished.body["end"], eq(&json!("handed_back")));
    expect_that!(finished.body["cost_usd"], eq(&json!(0.2)));
    let entry = entries
        .iter()
        .find(|(action, _)| action == "turn_handed_back")
        .context("no turn_handed_back entry")?;
    expect_that!(
        entry.1,
        eq(&json!({ "goal": "with_context", "reason": "Already has its context" }))
    );
    let made_in: Vec<Option<&str>> = runs
        .iter()
        .filter(|(action, _)| action == "turn_handed_back")
        .map(|(_, run_id)| run_id.as_deref())
        .collect();
    expect_that!(made_in, elements_are![some(eq(&run.as_str()))]);
    // The history reads as a sentence with labelled fields.
    let latest = &history.body["entries"][0];
    expect_that!(latest["phrase"], eq(&json!("handed back a turn on")));
    expect_that!(
        latest["changes"],
        eq(&json!([
            { "field": "goal", "label": "Goal", "before": null, "after": "With context" },
            { "field": "reason", "label": "Reason", "before": null, "after": "Already has its context" },
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_hand_back_needs_a_reason_and_the_callers_own_run() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let (other, _) = w.app.register("bob@example.test").await?;

    // Act
    let empty = w.run_call(&run, "hand-back", Some(json!({ "reason": "  " }))).await?;
    let long = w
        .run_call(&run, "hand-back", Some(json!({ "reason": "x".repeat(2_001) })))
        .await?;
    let theirs = w
        .app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{run}/hand-back"),
            Some(&other),
            Some(json!({ "reason": "Mine now" })),
        )
        .await?;
    let still_live = w.run_call(&run, "heartbeat", None).await?;

    // Assert
    expect_that!(empty.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(empty.body["field"], eq(&json!("reason")));
    expect_that!(long.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(long.body["field"], eq(&json!("reason")));
    expect_that!(theirs.status, eq(StatusCode::NOT_FOUND));
    expect_that!(still_live.status, eq(StatusCode::NO_CONTENT));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn two_claims_at_once_never_get_the_same_item() -> Result<()> {
    // Arrange: two items for the agents, and one.
    let w = world().await?;
    let first = w.add_sent("Rotate the certs").await?;
    let second = w.add_sent("Renew the domain").await?;

    // Act
    let (a, b) = tokio::join!(w.claim(), w.claim());
    let (a, b) = (a?, b?);

    // Assert
    expect_that!(a.status, eq(StatusCode::CREATED));
    expect_that!(b.status, eq(StatusCode::CREATED));
    let mut keys = vec![
        a.body["run"]["item"]["key"].as_str().context("key")?.to_owned(),
        b.body["run"]["item"]["key"].as_str().context("key")?.to_owned(),
    ];
    keys.sort();
    let mut both = vec![first, second];
    both.sort();
    expect_that!(keys, eq(&both));
    expect_that!(w.app.count_rows("item_runs").await?, eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn two_claims_at_once_on_one_item_give_it_to_one() -> Result<()> {
    // Arrange
    let w = world().await?;
    let _key = w.add_sent("Rotate the certs").await?;

    // Act
    let (a, b) = tokio::join!(w.claim(), w.claim());
    let (a, b) = (a?, b?);

    // Assert
    let claimed = [&a, &b].into_iter().filter(|r| r.status == StatusCode::CREATED).count();
    let empty = [&a, &b]
        .into_iter()
        .filter(|r| r.status == StatusCode::OK && r.body["run"].is_null())
        .count();
    expect_that!(claimed, eq(1));
    expect_that!(empty, eq(1));
    expect_that!(w.app.count_rows("item_runs").await?, eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn three_lapsed_runs_make_the_turn_stuck_and_a_retry_puts_it_back() -> Result<()> {
    // Arrange: an item whose runs go quiet.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let first = w.claimed().await?;

    // Act: the run lapses, so the queue offers the item again, and a claim writes the lapse.
    w.app.clock.advance(Duration::from_minutes(30));
    let offered = w.next().await?;
    let second = w.claimed().await?;
    let still_first_failed = w.finish(&first, json!({})).await?;
    w.app.clock.advance(Duration::from_minutes(31));
    let third = w.claimed().await?;
    // The third run lapses and nobody claims again, so the lapse isn't written: it still counts as failed.
    w.app.clock.advance(Duration::from_minutes(31));
    let queue = w.next().await?;
    let item = w.get(&key).await?;
    let waiting = w.waiting().await?;
    let retried = w
        .app
        .send(
            Method::POST,
            &format!("{}/turn/retry", w.item_path(&key).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let back = w.next().await?;
    let again = w
        .app
        .send(
            Method::POST,
            &format!("{}/turn/retry", w.item_path(&key).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let entries = w.app.audit_entries(item["id"].as_str().context("id")?).await?;

    // Assert
    expect_that!(offered.body["item"]["key"], eq(&json!(key)));
    expect_that!(second, not(eq(&first)));
    // The first run was written as failed by the second claim; finish records nothing more.
    expect_that!(still_first_failed.body["end"], eq(&json!("failed")));
    expect_that!(
        still_first_failed.body["failure"],
        eq(&json!("Nothing was heard from the run for 30 minutes."))
    );
    expect_that!(third, not(eq(&second)));
    expect_that!(queue.body["item"], eq(&Value::Null));
    expect_that!(queue.body["waiting_count"], eq(&json!(1)));
    expect_that!(item["turn"]["stuck"], eq(true));
    expect_that!(item["turn"]["failed_runs"], eq(&json!(3)));
    expect_that!(item["turn"]["runs"], eq(&json!(3)));
    expect_that!(
        item["turn"]["stuck_reason"],
        eq(&json!("Nothing was heard from the run for 30 minutes."))
    );
    expect_that!(item["waits_for_you"], eq(true));
    expect_that!(item["with_agent"], eq(false));
    expect_that!(actions(&item), contains(eq(&"retry")));
    expect_that!(actions(&item), contains(eq(&"take_back")));
    expect_that!(waiting["count"], eq(&json!(1)));
    let row = &waiting["rows"][0];
    expect_that!(row["kind"], eq(&json!("stuck")));
    expect_that!(row["question"], eq(&Value::Null));
    expect_that!(row["stuck"]["failed_runs"], eq(&json!(3)));
    expect_that!(row["stuck"]["goal"], eq(&json!("with_context")));
    expect_that!(
        row["stuck"]["reason"],
        eq(&json!("Nothing was heard from the run for 30 minutes."))
    );
    expect_that!(row["stuck"]["allowed_actions"], eq(&json!(["retry", "take_back"])));
    expect_that!(retried.status, eq(StatusCode::OK));
    expect_that!(retried.body["turn"]["stuck"], eq(false));
    expect_that!(retried.body["turn"]["failed_runs"], eq(&json!(0)));
    expect_that!(retried.body["waits_for_you"], eq(false));
    expect_that!(retried.body["with_agent"], eq(true));
    expect_that!(back.body["item"]["key"], eq(&json!(key)));
    expect_that!(back.body["item"]["failed_runs"], eq(&json!(0)));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&again), some(eq("turn_not_stuck")));
    let retries = entries.iter().filter(|(action, _)| action == "turn_retried").count();
    expect_that!(retries, eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn retrying_a_turn_that_is_not_stuck_or_has_no_turn_is_refused() -> Result<()> {
    // Arrange
    let w = world().await?;
    let sent = w.add_sent("Has a turn").await?;
    let plain = w.add("No turn", json!({})).await?;

    // Act
    let mut results = Vec::new();
    for key in [&sent, &plain] {
        results.push(
            w.app
                .send(
                    Method::POST,
                    &format!("{}/turn/retry", w.item_path(key).await?),
                    Some(&w.session),
                    None,
                )
                .await?,
        );
    }

    // Assert
    for response in &results {
        expect_that!(response.status, eq(StatusCode::CONFLICT));
        expect_that!(code(response), some(eq("turn_not_stuck")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_belongs_to_the_user_who_claimed_it() -> Result<()> {
    // Arrange: a shared heap with a second member.
    let app = TestApp::start().await?;
    let (admin, _) = app.register("ada@example.test").await?;
    let (member, member_heap) = app.register("bob@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    app.add_member(
        &team,
        me.body["user"]["id"].as_str().context("id")?,
        domain::team::Role::Member,
    )
    .await?;
    let _ = member_heap;
    let heap_ref = heap_ref(&app, &heap).await?;
    let w = World {
        app,
        session: admin.clone(),
        heap,
        heap_ref,
    };
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;

    // Act
    let by_member = w
        .app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{run}/finish"),
            Some(&member),
            Some(json!({})),
        )
        .await?;
    let write_by_member = w
        .app
        .send_with(
            Method::PATCH,
            &w.item_path(&key).await?,
            Some(&member),
            Some(json!({ "description": "Mine now" })),
            &[CSRF, ("upheap-run", &run), ("if-match", "1")],
        )
        .await?;

    // Assert
    expect_that!(by_member.status, eq(StatusCode::NOT_FOUND));
    expect_that!(write_by_member.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_header_that_is_not_a_run_id_is_a_bad_request_even_on_a_read() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add("Rotate the certs", json!({})).await?;

    // Act
    let write = w
        .app
        .send_with(
            Method::POST,
            &format!("{}/steps", w.item_path(&key).await?),
            Some(&w.session),
            Some(json!({ "title": "A step" })),
            &[CSRF, ("upheap-run", "not-a-run")],
        )
        .await?;
    let read = w
        .app
        .send_with(
            Method::GET,
            &w.item_path(&key).await?,
            Some(&w.session),
            None,
            &[CSRF, ("upheap-run", "not-a-run")],
        )
        .await?;

    // Assert
    expect_that!(write.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(code(&write), some(eq("invalid_header")));
    expect_that!(read.status, eq(StatusCode::BAD_REQUEST));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn closing_an_item_by_hand_mid_run_takes_it_back_and_ends_the_run() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;

    // Act
    let closed = w.edit(&key, json!({ "state": "done" })).await?;
    let finished = w.finish(&run, json!({})).await?;

    // Assert
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(closed.body["turn"], eq(&Value::Null));
    expect_that!(finished.body["end"], eq(&json!("taken_back")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archiving_the_heap_ends_live_runs_as_taken_back() -> Result<()> {
    // Arrange
    let w = world().await?;
    let _key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;
    let path = w.app.heap_url(&w.heap).await?;
    let heap = w.app.send(Method::GET, &path, Some(&w.session), None).await?;

    // Act
    let archived = w
        .app
        .send_with(
            Method::POST,
            &format!("{path}/archive"),
            Some(&w.session),
            None,
            &[CSRF, ("if-match", &heap.body["version"].to_string())],
        )
        .await?;
    let beat = w.run_call(&run, "heartbeat", None).await?;
    let finished = w.finish(&run, json!({ "cost_usd": 0.05 })).await?;

    // Assert
    expect_that!(archived.status.is_success(), eq(true), "{}", archived.body);
    expect_that!(beat.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&beat), some(eq("run_over")));
    expect_that!(finished.body["end"], eq(&json!("taken_back")));
    expect_that!(finished.body["cost_usd"], eq(&json!(0.05)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_cost_is_set_once() -> Result<()> {
    // Arrange
    let w = world().await?;
    let _key = w.add_sent("Rotate the certs").await?;
    let run = w.claimed().await?;

    // Act
    let first = w.finish(&run, json!({ "cost_usd": 0.1234567 })).await?;
    let second = w.finish(&run, json!({ "cost_usd": 9 })).await?;
    let without = w.finish(&run, json!({})).await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::OK));
    // Kept in whole millionths of a dollar.
    expect_that!(first.body["cost_usd"], eq(&json!(0.123457)));
    expect_that!(second.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&second), some(eq("run_cost_recorded")));
    expect_that!(without.status, eq(StatusCode::OK));
    expect_that!(without.body["cost_usd"], eq(&json!(0.123457)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stuck_item_leaves_waiting_when_it_is_closed() -> Result<()> {
    // Arrange: a stuck turn.
    let w = world().await?;
    let key = w.add_sent("Rotate the certs").await?;
    for _ in 0..3 {
        w.fail_once().await?;
    }
    let before = w.waiting().await?;

    // Act
    let closed = w.edit(&key, json!({ "state": "wont_do" })).await?;
    let after = w.waiting().await?;

    // Assert
    expect_that!(before["count"], eq(&json!(1)));
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(after["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_comments_on_its_own_item_and_not_on_another() -> Result<()> {
    // Arrange: two items; the run holds the more important one.
    let w = world().await?;
    let mine = w.add("Mine", json!({ "need": "critical" })).await?;
    w.send_turn(&mine).await?;
    let other = w.add("Other", json!({})).await?;
    let run = w.claimed().await?;

    // Act
    let own = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/comments", w.item_path(&mine).await?),
            None,
            Some(json!({ "body": "Found the staging certificate." })),
        )
        .await?;
    let wrong_item = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/comments", w.item_path(&other).await?),
            None,
            Some(json!({ "body": "Not mine" })),
        )
        .await?;

    // Assert
    expect_that!(own.status, eq(StatusCode::CREATED));
    expect_that!(own.body["comments"][0]["number"], eq(&json!(1)));
    expect_that!(wrong_item.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&wrong_item), some(eq("run_wrong_item")));
    expect_that!(w.get(&other).await?["comments"], eq(&json!([])));
    Ok(())
}
