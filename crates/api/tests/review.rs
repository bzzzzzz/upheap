//! Reviews, done proposals and redo: who may ask, the rounds, what a proposal needs, what agreeing to one does, and
//! asking for changes to a turn that reached a stable state.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

const PR1: &str = "https://example.test/pull/1";
const PR2: &str = "https://example.test/pull/2";

/// A user with a personal heap.
struct World {
    app: TestApp,
    session: String,
    heap: String,
    heap_ref: String,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let url = app.heap_url(&heap).await?;
    let heap_ref = url
        .strip_prefix("/api/v1/teams/")
        .context("heap url")?
        .replace("/heaps/", "/");
    Ok(World {
        app,
        session,
        heap,
        heap_ref,
    })
}

fn code(response: &TestResponse) -> Option<&str> {
    response.body["code"].as_str()
}

fn title(response: &TestResponse) -> &str {
    response.body["title"].as_str().unwrap_or_default()
}

impl World {
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

    /// A task with context and `steps` steps, defined, so a turn sent now goes to done. Returns its key.
    async fn defined_task(&self, title: &str, steps: usize) -> Result<String> {
        let item = self
            .app
            .new_item(
                &self.session,
                &self.heap,
                json!({ "title": title, "description": "Why and how" }),
            )
            .await?;
        let key = item["key"].as_str().context("no key")?.to_owned();
        let made = self.edit(&key, json!({ "kind": "task" })).await?;
        anyhow::ensure!(made.status == StatusCode::OK, "to task failed: {}", made.body);
        for n in 1..=steps {
            let step = self
                .app
                .send(
                    Method::POST,
                    &format!("{}/steps", self.item_path(&key).await?),
                    Some(&self.session),
                    Some(json!({ "title": format!("Step {n}") })),
                )
                .await?;
            anyhow::ensure!(step.status == StatusCode::CREATED, "step failed: {}", step.body);
        }
        let defined = self.edit(&key, json!({ "stage": "defined" })).await?;
        anyhow::ensure!(defined.status == StatusCode::OK, "stage failed: {}", defined.body);
        Ok(key)
    }

    async fn send_turn(&self, key: &str) -> Result<()> {
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{}/turn", self.item_path(key).await?),
                Some(&self.session),
                Some(json!({ "reach": "one" })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        Ok(())
    }

    async fn claimed(&self) -> Result<String> {
        let claimed = self
            .app
            .send(
                Method::POST,
                "/api/v1/work/runs",
                Some(&self.session),
                Some(json!({ "heaps": [self.heap_ref] })),
            )
            .await?;
        anyhow::ensure!(claimed.status == StatusCode::CREATED, "claim failed: {}", claimed.body);
        Ok(claimed.body["run"]["id"].as_str().context("no run")?.to_owned())
    }

    async fn finish(&self, run: &str) -> Result<()> {
        let finished = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/work/runs/{run}/finish"),
                Some(&self.session),
                Some(json!({})),
            )
            .await?;
        anyhow::ensure!(finished.status == StatusCode::OK, "finish failed: {}", finished.body);
        Ok(())
    }

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

    async fn ask_in_run(&self, run: &str, key: &str, body: Value) -> Result<TestResponse> {
        self.in_run(
            run,
            Method::POST,
            &format!("{}/questions", self.item_path(key).await?),
            None,
            Some(body),
        )
        .await
    }

    async fn review(&self, run: &str, key: &str, link: &str, piece: Option<&str>) -> Result<TestResponse> {
        let mut body = json!({ "kind": "review", "text": "The change", "link": link });
        if let Some(piece) = piece {
            body["piece"] = json!(piece);
        }
        self.ask_in_run(run, key, body).await
    }

    async fn propose_done(&self, run: &str, key: &str) -> Result<TestResponse> {
        self.ask_in_run(run, key, json!({ "kind": "done_proposal", "text": "Nothing is left" }))
            .await
    }

    /// The person's reply to question `number`, picking `pick`.
    async fn reply(&self, key: &str, number: u64, pick: u32, comment: Option<&str>) -> Result<TestResponse> {
        let item = self.get(key).await?;
        let question = item["questions"]
            .as_array()
            .and_then(|all| all.iter().find(|q| q["number"] == json!(number)))
            .context("no such question")?;
        let mut reply = json!({ "picked": [pick] });
        if let Some(comment) = comment {
            reply["comment"] = json!(comment);
        }
        self.app
            .patch(
                &self.session,
                &format!("{}/questions/{number}", self.item_path(key).await?),
                &question["version"],
                json!({ "reply": reply }),
            )
            .await
    }

    async fn tick_all_steps(&self, key: &str) -> Result<()> {
        let item = self.get(key).await?;
        for step in item["steps"].as_array().context("steps")? {
            let ticked = self
                .app
                .patch(
                    &self.session,
                    &format!("{}/steps/{}", self.item_path(key).await?, step["number"]),
                    &step["version"],
                    json!({ "state": "ticked" }),
                )
                .await?;
            anyhow::ensure!(ticked.status == StatusCode::OK, "tick failed: {}", ticked.body);
        }
        Ok(())
    }

    /// A task with a done turn and a live run on it. Returns the key and the run.
    async fn task_in_a_done_run(&self, steps: usize) -> Result<(String, String)> {
        let key = self.defined_task("Paint the fence", steps).await?;
        self.send_turn(&key).await?;
        let run = self.claimed().await?;
        Ok((key, run))
    }
}

fn offered(item: &Value) -> Vec<&str> {
    item["allowed_actions"]
        .as_array()
        .map(|all| all.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn question(item: &Value, number: u64) -> &Value {
    item["questions"]
        .as_array()
        .and_then(|all| all.iter().find(|q| q["number"] == json!(number)))
        .unwrap_or(&Value::Null)
}

#[gtest]
#[tokio::test]
async fn a_review_asked_in_a_done_run_has_its_link_piece_and_round() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;

    // Act
    let by_link = w.review(&run, &key, PR1, None).await?;
    let named = w.review(&run, &key, PR2, Some("  the fence  ")).await?;

    // Assert
    expect_that!(by_link.status, eq(StatusCode::CREATED));
    let first = question(&by_link.body, 1);
    expect_that!(first["kind"], eq(&json!("review")));
    expect_that!(first["choices"], eq(&json!(["Approved", "Changes asked"])));
    expect_that!(first["review"], eq(&json!({ "link": PR1, "piece": PR1, "round": 1 })));
    expect_that!(
        question(&named.body, 2)["review"],
        eq(&json!({ "link": PR2, "piece": "the fence", "round": 1 }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_review_needs_a_done_run_on_a_task_with_a_link() -> Result<()> {
    // Arrange: a person's hand, a run that goes to defined, and a note.
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    let by_hand = w
        .app
        .send(
            Method::POST,
            &format!("{}/questions", w.item_path(&key).await?),
            Some(&w.session),
            Some(json!({ "kind": "review", "text": "x", "link": PR1 })),
        )
        .await?;

    // Act
    let no_link = w
        .ask_in_run(&run, &key, json!({ "kind": "review", "text": "x" }))
        .await?;
    let bad_link = w.review(&run, &key, "ftp://example.test/x", None).await?;
    let link_on_request = w
        .ask_in_run(&run, &key, json!({ "kind": "request", "text": "x", "link": PR1 }))
        .await?;
    let with_step = w
        .ask_in_run(
            &run,
            &key,
            json!({ "kind": "review", "text": "x", "link": PR1, "step": 1 }),
        )
        .await?;

    // Assert
    expect_that!(by_hand.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&by_hand), some(eq("question_needs_do_turn")));
    expect_that!(no_link.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(bad_link.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(link_on_request.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(with_step.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_review_in_a_run_that_goes_to_defined_is_refused() -> Result<()> {
    // Arrange: a task that is not defined yet, so its turn goes to defined.
    let w = world().await?;
    let task = w
        .app
        .new_item(
            &w.session,
            &w.heap,
            json!({ "title": "Not yet defined", "description": "Why" }),
        )
        .await?;
    let key = task["key"].as_str().context("key")?.to_owned();
    let made = w.edit(&key, json!({ "kind": "task" })).await?;
    anyhow::ensure!(made.status == StatusCode::OK, "{}", made.body);
    w.send_turn(&key).await?;
    let run = w.claimed().await?;

    // Act
    let review = w.review(&run, &key, PR1, None).await?;
    let proposal = w.propose_done(&run, &key).await?;

    // Assert
    expect_that!(review.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&review), some(eq("question_needs_do_turn")));
    expect_that!(code(&proposal), some(eq("question_needs_do_turn")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_review_on_a_note_is_refused() -> Result<()> {
    // Arrange: a note with a turn, claimed.
    let w = world().await?;
    let note = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "A note", "description": "Why" }))
        .await?;
    let key = note["key"].as_str().context("key")?.to_owned();
    w.send_turn(&key).await?;
    let run = w.claimed().await?;

    // Act
    let review = w.review(&run, &key, PR1, None).await?;
    let proposal = w.propose_done(&run, &key).await?;

    // Assert
    expect_that!(review.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&review), some(eq("question_needs_task")));
    expect_that!(code(&proposal), some(eq("question_needs_task")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_second_review_of_a_piece_waits_but_another_piece_may_be_open() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    w.review(&run, &key, PR1, None).await?;

    // Act
    let same = w.review(&run, &key, PR1, None).await?;
    let other = w.review(&run, &key, PR2, None).await?;

    // Assert
    expect_that!(same.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&same), some(eq("review_open")));
    expect_that!(other.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn asking_again_after_changes_asked_is_the_next_round() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    w.review(&run, &key, PR1, None).await?;
    w.finish(&run).await?;
    let changes = w.reply(&key, 1, 1, Some("The corner is missed.")).await?;
    let again = w.claimed().await?;

    // Act
    let second = w.review(&again, &key, PR1, None).await?;

    // Assert
    expect_that!(changes.status, eq(StatusCode::OK));
    expect_that!(changes.body["with_agent"], eq(true));
    expect_that!(changes.body["state"], eq(&json!("open")));
    expect_that!(second.status, eq(StatusCode::CREATED));
    expect_that!(question(&second.body, 2)["review"]["round"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_back_review_does_not_count_toward_the_round() -> Result<()> {
    // Arrange: a review asked, then the turn taken back, which withdraws it.
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    w.review(&run, &key, PR1, None).await?;
    let back = w
        .app
        .send(
            Method::DELETE,
            &format!("{}/turn", w.item_path(&key).await?),
            Some(&w.session),
            None,
        )
        .await?;
    w.send_turn(&key).await?;
    let again = w.claimed().await?;

    // Act
    let second = w.review(&again, &key, PR1, None).await?;

    // Assert
    expect_that!(back.status, eq(StatusCode::OK));
    expect_that!(question(&back.body, 1)["state"], eq(&json!("withdrawn")));
    expect_that!(question(&second.body, 2)["review"]["round"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_proposal_is_refused_naming_each_thing_that_is_left() -> Result<()> {
    // Arrange: two open steps, an open review of one piece, and a second piece whose latest round wasn't approved.
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(2).await?;
    w.review(&run, &key, PR2, None).await?;
    w.finish(&run).await?;
    w.reply(&key, 1, 1, None).await?;
    let next = w.claimed().await?;

    // Act
    let unapproved = w.propose_done(&next, &key).await?;
    w.review(&next, &key, PR1, None).await?;
    let all = w.propose_done(&next, &key).await?;

    // Assert
    expect_that!(unapproved.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&unapproved), some(eq("not_ready_to_close")));
    expect_that!(title(&unapproved), contains_substring("2 open steps"));
    expect_that!(
        title(&unapproved),
        contains_substring(format!("the piece {PR2} has round 1 not approved"))
    );
    expect_that!(title(&all), contains_substring("2 open steps"));
    expect_that!(title(&all), contains_substring("review q2 is still open"));
    expect_that!(
        title(&all),
        contains_substring(format!("the piece {PR1} has round 1 not approved"))
    );
    expect_that!(
        title(&all),
        contains_substring(format!("the piece {PR2} has round 1 not approved"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_proposal_with_an_open_request_is_refused() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(0).await?;
    w.ask_in_run(&run, &key, json!({ "kind": "request", "text": "Add the secret" }))
        .await?;

    // Act
    let proposal = w.propose_done(&run, &key).await?;

    // Assert
    expect_that!(code(&proposal), some(eq("not_ready_to_close")));
    expect_that!(title(&proposal), contains_substring("request q1 is still open"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_agent_never_closes_a_task() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    let version = w.get(&key).await?["version"].clone();
    let path = w.item_path(&key).await?;

    // Act
    let done = w
        .in_run(
            &run,
            Method::PATCH,
            &path,
            Some(&version),
            Some(json!({ "state": "done" })),
        )
        .await?;
    let wont = w
        .in_run(
            &run,
            Method::PATCH,
            &path,
            Some(&version),
            Some(json!({ "state": "wont_do" })),
        )
        .await?;

    // Assert
    expect_that!(done.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&done), some(eq("run_done_needs_review")));
    expect_that!(title(&done), contains_substring("propose-done"));
    expect_that!(wont.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&wont), some(eq("run_forbidden")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn two_reviews_one_approved_end_to_end() -> Result<()> {
    // Arrange: a defined task with two steps that blocks another item, and a done turn.
    let w = world().await?;
    let key = w.defined_task("Paint the fence", 2).await?;
    let blocked = w
        .app
        .new_item(&w.session, &w.heap, json!({ "title": "Hang the gate" }))
        .await?;
    let blocked_key = blocked["key"].as_str().context("key")?.to_owned();
    let task_id = w.get(&key).await?["id"].clone();
    let blocked_ok = w
        .edit(
            &blocked_key,
            json!({ "state": "blocked", "blocked_by_item_id": task_id }),
        )
        .await?;
    anyhow::ensure!(blocked_ok.status == StatusCode::OK, "block failed: {}", blocked_ok.body);
    w.send_turn(&key).await?;

    // Act: review PR1 and PR2; the person approves PR1 and asks changes on PR2.
    let run1 = w.claimed().await?;
    let r1 = w.review(&run1, &key, PR1, None).await?;
    let r2 = w.review(&run1, &key, PR2, None).await?;
    w.finish(&run1).await?;
    let approved = w.reply(&key, 1, 0, None).await?;
    let waiting_between = w.get(&key).await?;
    let changes = w.reply(&key, 2, 1, Some("Rename it.")).await?;
    let in_queue = changes.body["with_agent"].clone();

    // Act: a proposal now is refused; review PR2 again, which is round 2.
    let run2 = w.claimed().await?;
    let refused = w.propose_done(&run2, &key).await?;
    let round2 = w.review(&run2, &key, PR2, None).await?;
    w.finish(&run2).await?;
    w.reply(&key, 3, 0, None).await?;

    // Act: tick the rest, propose, and the person agrees.
    w.tick_all_steps(&key).await?;
    let run3 = w.claimed().await?;
    let proposal = w.propose_done(&run3, &key).await?;
    w.finish(&run3).await?;
    let closed = w.reply(&key, 4, 0, Some("Thanks")).await?;

    // Assert
    expect_that!(question(&r1.body, 1)["review"]["round"], eq(&json!(1)));
    expect_that!(question(&r2.body, 2)["review"]["round"], eq(&json!(1)));
    expect_that!(approved.body["with_agent"], eq(false));
    expect_that!(waiting_between["waits_for_you"], eq(true));
    expect_that!(in_queue, eq(&json!(true)));
    expect_that!(code(&refused), some(eq("not_ready_to_close")));
    expect_that!(
        title(&refused),
        contains_substring(format!("the piece {PR2} has round 1 not approved"))
    );
    expect_that!(question(&round2.body, 4 - 1)["review"]["round"], eq(&json!(2)));
    expect_that!(proposal.status, eq(StatusCode::CREATED));
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(closed.body["state"], eq(&json!("done")));
    expect_that!(closed.body["turn"], eq(&Value::Null));
    expect_that!(closed.body["with_agent"], eq(false));
    expect_that!(closed.body["closed_by"]["display_name"].is_string(), eq(true));
    let id = closed.body["id"].as_str().context("id")?;
    let entries = w.app.audit_entries(id).await?;
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    expect_that!(actions, contains(eq(&"turn_reached")));
    expect_that!(actions, not(contains(eq(&"turn_taken_back"))));
    let freed = w.get(&blocked_key).await?;
    expect_that!(freed["state"], eq(&json!("open")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_with_no_reviews_closes_from_a_proposal_alone() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    w.tick_all_steps(&key).await?;
    let proposal = w.propose_done(&run, &key).await?;
    w.finish(&run).await?;

    // Act
    let closed = w.reply(&key, 1, 0, None).await?;

    // Assert
    expect_that!(proposal.status, eq(StatusCode::CREATED));
    expect_that!(closed.status, eq(StatusCode::OK));
    expect_that!(closed.body["state"], eq(&json!("done")));
    expect_that!(closed.body["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn not_yet_returns_the_task_to_the_agent_with_the_comment() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(0).await?;
    w.propose_done(&run, &key).await?;
    w.finish(&run).await?;

    // Act
    let declined = w.reply(&key, 1, 1, Some("Also update the README.")).await?;

    // Assert
    expect_that!(declined.status, eq(StatusCode::OK));
    expect_that!(declined.body["state"], eq(&json!("open")));
    expect_that!(declined.body["with_agent"], eq(true));
    expect_that!(
        question(&declined.body, 1)["reply"]["comment"],
        eq(&json!("Also update the README."))
    );
    let next = w
        .app
        .send(
            Method::GET,
            &format!("/api/v1/work/next?heaps={}", w.heap_ref),
            Some(&w.session),
            None,
        )
        .await?;
    expect_that!(next.body["item"]["key"], eq(&json!(key)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn waiting_for_me_shows_a_review_with_its_link_and_round() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(1).await?;
    w.review(&run, &key, PR1, None).await?;
    w.finish(&run).await?;

    // Act
    let waiting = w
        .app
        .send(Method::GET, "/api/v1/me/waiting", Some(&w.session), None)
        .await?;

    // Assert
    expect_that!(waiting.body["count"], eq(&json!(1)));
    expect_that!(
        waiting.body["rows"][0]["question"]["review"],
        eq(&json!({ "link": PR1, "piece": PR1, "round": 1 }))
    );
    Ok(())
}

/// A note with a context, sent to an agent, reached by a run. Returns its key.
async fn reached_note(w: &World) -> Result<String> {
    let note = w
        .app
        .new_item(
            &w.session,
            &w.heap,
            json!({ "title": "Rotate the certs", "description": "Why" }),
        )
        .await?;
    let key = note["key"].as_str().context("key")?.to_owned();
    w.send_turn(&key).await?;
    let run = w.claimed().await?;
    let reached = w
        .app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{run}/reached"),
            Some(&w.session),
            Some(json!({ "result": "Added the context" })),
        )
        .await?;
    anyhow::ensure!(reached.status == StatusCode::OK, "reached failed: {}", reached.body);
    Ok(key)
}

async fn redo(w: &World, key: &str, line: &str) -> Result<TestResponse> {
    w.app
        .send(
            Method::POST,
            &format!("{}/turn/redo", w.item_path(key).await?),
            Some(&w.session),
            Some(json!({ "line": line })),
        )
        .await
}

#[gtest]
#[tokio::test]
async fn asking_for_changes_reopens_a_turn_to_the_same_goal_at_the_earlier_stage() -> Result<()> {
    // Arrange
    let w = world().await?;
    let key = reached_note(&w).await?;
    let before = w.get(&key).await?;

    // Act
    let redone = redo(&w, &key, "Use the new CA.").await?;

    // Assert
    expect_that!(before["stage"], eq(&json!("with_context")));
    expect_that!(offered(&before), contains(eq(&"redo_turn")));
    expect_that!(redone.status, eq(StatusCode::CREATED));
    expect_that!(redone.body["stage"], eq(&json!("as_written")));
    expect_that!(redone.body["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(redone.body["turn"]["line"], eq(&json!("Use the new CA.")));
    expect_that!(redone.body["turn"]["source"], eq(&json!("go_ahead")));
    expect_that!(offered(&redone.body), not(contains(eq(&"redo_turn"))));
    let id = redone.body["id"].as_str().context("id")?;
    let entries = w.app.audit_entries(id).await?;
    let actions: Vec<&str> = entries.iter().map(|(action, _)| action.as_str()).collect();
    let tail = &actions[actions.len().saturating_sub(3)..];
    expect_that!(tail, eq(&["turn_reached", "turn_redone", "turn_opened"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn there_is_nothing_to_redo_with_an_open_turn_a_taken_back_one_or_a_repeat() -> Result<()> {
    // Arrange
    let w = world().await?;
    let reached = reached_note(&w).await?;
    redo(&w, &reached, "Again").await?;
    let sent = w.defined_task("Has an open turn", 0).await?;
    w.send_turn(&sent).await?;
    let taken_back = w.defined_task("Taken back", 0).await?;
    w.send_turn(&taken_back).await?;
    w.app
        .send(
            Method::DELETE,
            &format!("{}/turn", w.item_path(&taken_back).await?),
            Some(&w.session),
            None,
        )
        .await?;

    // Act
    let open = redo(&w, &sent, "x").await?;
    let back = redo(&w, &taken_back, "x").await?;
    let twice = redo(&w, &reached, "x").await?;
    let blank = redo(&w, &reached, "  ").await?;

    // Assert
    for refused in [&open, &back, &twice] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(code(refused), some(eq("nothing_to_redo")));
    }
    expect_that!(blank.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_agent_in_a_run_cannot_ask_for_changes() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (key, run) = w.task_in_a_done_run(0).await?;

    // Act
    let refused = w
        .in_run(
            &run,
            Method::POST,
            &format!("{}/turn/redo", w.item_path(&key).await?),
            None,
            Some(json!({ "line": "x" })),
        )
        .await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&refused), some(eq("run_forbidden")));
    Ok(())
}
