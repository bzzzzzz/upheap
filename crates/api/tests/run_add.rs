//! A run adding items: what a run may add and where, what it is refused, the steps it may add to what it added, and
//! what the new item records of the run.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// A user with a held item (`held`, a key) in their personal heap, and a live run on it.
struct Scene {
    app: TestApp,
    session: String,
    heap: String,
    held: String,
    run: String,
    /// The heap as `team/heap`, as a claim names it.
    heap_ref: String,
}

async fn scene() -> Result<Scene> {
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let held = app.new_item(&session, &heap, json!({ "title": "Held" })).await?;
    let held = held["key"].as_str().context("no key")?.to_owned();
    let path = format!("{}/items/{held}/turn", app.heap_url(&heap).await?);
    let sent = app
        .send(Method::POST, &path, Some(&session), Some(json!({ "reach": "one" })))
        .await?;
    anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
    let heap_ref = app
        .heap_url(&heap)
        .await?
        .strip_prefix("/api/v1/teams/")
        .context("heap url")?
        .replace("/heaps/", "/");
    let run = claim(&app, &session, &heap_ref).await?;
    Ok(Scene {
        app,
        session,
        heap,
        held,
        run,
        heap_ref,
    })
}

/// Claims a run in the heap and returns its ID.
async fn claim(app: &TestApp, session: &str, heap_ref: &str) -> Result<String> {
    let claimed = app
        .send(
            Method::POST,
            "/api/v1/work/runs",
            Some(session),
            Some(json!({ "heaps": [heap_ref] })),
        )
        .await?;
    anyhow::ensure!(claimed.status == StatusCode::CREATED, "claim failed: {}", claimed.body);
    Ok(claimed.body["run"]["id"].as_str().context("no run")?.to_owned())
}

impl Scene {
    /// A request in the run.
    async fn in_run(&self, method: Method, path: &str, body: Option<Value>) -> Result<TestResponse> {
        self.in_run_as(&self.run, method, path, body).await
    }

    async fn in_run_as(&self, run: &str, method: Method, path: &str, body: Option<Value>) -> Result<TestResponse> {
        self.app
            .send_with(method, path, Some(&self.session), body, &[CSRF, ("upheap-run", run)])
            .await
    }

    /// A patch in the run, with the item's current version.
    async fn patch_in_run(&self, path: &str, body: Value) -> Result<TestResponse> {
        let current = self.app.send(Method::GET, path, Some(&self.session), None).await?;
        let version = current.body["version"].to_string();
        self.app
            .send_with(
                Method::PATCH,
                path,
                Some(&self.session),
                Some(body),
                &[CSRF, ("upheap-run", &self.run), ("if-match", &version)],
            )
            .await
    }

    async fn add_to(&self, heap: &str, mut body: Value) -> Result<TestResponse> {
        if let Value::Object(fields) = &mut body {
            fields.entry("need").or_insert_with(|| Value::from("expected"));
        }
        let path = format!("{}/items", self.app.heap_url(heap).await?);
        self.in_run(Method::POST, &path, Some(body)).await
    }

    async fn add(&self, body: Value) -> Result<TestResponse> {
        self.add_to(&self.heap, body).await
    }

    async fn path_of(&self, heap: &str, key: &str) -> Result<String> {
        Ok(format!("{}/items/{key}", self.app.heap_url(heap).await?))
    }
}

fn code(response: &TestResponse) -> Option<&str> {
    response.body["code"].as_str()
}

async fn set_prepare(app: &TestApp, session: &str, heap: &str) -> Result<()> {
    let path = app.heap_url(heap).await?;
    let current = app.send(Method::GET, &path, Some(session), None).await?;
    let patched = app
        .patch(
            session,
            &path,
            &current.body["version"],
            json!({ "agents_prepare": true }),
        )
        .await?;
    anyhow::ensure!(patched.status == StatusCode::OK, "prepare failed: {}", patched.body);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_adds_a_note_with_a_context_to_its_own_heap_and_no_turn_opens() -> Result<()> {
    // Arrange: the heap prepares what is added.
    let s = scene().await?;
    set_prepare(&s.app, &s.session, &s.heap).await?;

    // Act
    let added = s
        .add(json!({ "title": "Rotate the certs", "description": "Came up while fixing the CA" }))
        .await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::CREATED));
    let item = &added.body;
    expect_that!(item["stage"], eq(&json!("with_context")));
    expect_that!(item["kind"], eq(&json!("note")));
    expect_that!(item["added_in_run"], eq(&json!(true)));
    expect_that!(item["origin"]["key"], eq(&json!(s.held)));
    expect_that!(item["origin"]["title"], eq(&json!("Held")));
    expect_that!(item["origin"]["kind"], eq(&json!("note")));
    expect_that!(item["origin"]["heap"]["id"], eq(&json!(s.heap)));
    expect_that!(item["turn"], eq(&Value::Null));
    let turns = s.app.count_rows("item_turns").await?;
    expect_that!(turns, eq(1));
    // The item reads the same later.
    let key = item["key"].as_str().context("key")?;
    let fetched = s
        .app
        .send(Method::GET, &s.path_of(&s.heap, key).await?, Some(&s.session), None)
        .await?;
    expect_that!(fetched.body["origin"]["key"], eq(&json!(s.held)));
    expect_that!(fetched.body["added_in_run"], eq(&json!(true)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_adds_a_task_with_every_field_to_another_heap_of_its_team_and_of_another_team() -> Result<()> {
    // Arrange: a second heap in the person's personal team, and a shared team they belong to.
    let s = scene().await?;
    let me = s.app.send(Method::GET, "/api/v1/me", Some(&s.session), None).await?;
    let team_slug = me.body["teams"][0]["slug"].as_str().context("slug")?.to_owned();
    let other = s
        .app
        .send(
            Method::POST,
            &format!("/api/v1/teams/{team_slug}/heaps"),
            Some(&s.session),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    anyhow::ensure!(other.status == StatusCode::CREATED, "heap failed: {}", other.body);
    let other_heap = other.body["id"].as_str().context("id")?.to_owned();
    let (_, shared_heap) = s.app.shared_heap(&s.session).await?;

    for heap in [&other_heap, &shared_heap] {
        let tag = s.app.new_tag_id(&s.session, heap, "ops").await?;
        let parameter = s
            .app
            .send(
                Method::POST,
                &format!("{}/parameters", s.app.heap_url(heap).await?),
                Some(&s.session),
                Some(json!({ "name": "Area", "options": ["Core", "Edge"] })),
            )
            .await?;
        anyhow::ensure!(parameter.status == StatusCode::CREATED, "parameter: {}", parameter.body);
        let parameter_id = parameter.body["id"].as_str().context("id")?;
        let option = parameter.body["options"][0]["id"].as_str().context("option")?;

        // Act
        let added = s
            .add_to(
                heap,
                json!({
                    "title": "Follow up",
                    "kind": "task",
                    "description": "Why it came up",
                    "need": "critical",
                    "effort": "s",
                    "impact": "l",
                    "due": { "date": "2027-03-12", "firm": true },
                    "tag_ids": [tag],
                    "parameter_values": { parameter_id: option },
                }),
            )
            .await?;

        // Assert
        expect_that!(added.status, eq(StatusCode::CREATED));
        let item = &added.body;
        expect_that!(item["kind"], eq(&json!("task")));
        expect_that!(item["stage"], eq(&json!("with_context")));
        expect_that!(item["need"], eq(&json!("critical")));
        expect_that!(item["effort"], eq(&json!("s")));
        expect_that!(item["impact"], eq(&json!("l")));
        expect_that!(item["due"]["date"], eq(&json!("2027-03-12")));
        expect_that!(item["tags"][0]["name"], eq(&json!("ops")));
        expect_that!(item["parameters"][0]["option"]["label"], eq(&json!("Core")));
        expect_that!(item["origin"]["key"], eq(&json!(s.held)));
        expect_that!(item["added_in_run"], eq(&json!(true)));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_is_refused_an_item_with_no_context_or_a_blank_one_or_a_reminder() -> Result<()> {
    // Arrange
    let s = scene().await?;

    // Act
    let none = s.add(json!({ "title": "No context" })).await?;
    let blank = s.add(json!({ "title": "Blank", "description": "  \n " })).await?;
    let reminder = s
        .add(json!({
            "title": "Remind",
            "kind": "reminder",
            "description": "Context",
            "due": { "date": "2027-03-12", "firm": true },
        }))
        .await?;

    // Assert
    expect_that!(none.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&none), some(eq("run_add_needs_context")));
    expect_that!(blank.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&blank), some(eq("run_add_needs_context")));
    expect_that!(reminder.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&reminder), some(eq("run_add_reminder")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_that_is_over_or_not_the_callers_adds_nothing() -> Result<()> {
    // Arrange
    let s = scene().await?;
    let turn = format!("{}/turn", s.path_of(&s.heap, &s.held).await?);
    let taken = s.app.send(Method::DELETE, &turn, Some(&s.session), None).await?;
    anyhow::ensure!(taken.status == StatusCode::OK, "take back failed: {}", taken.body);

    // Act
    let over = s.add(json!({ "title": "Late", "description": "Context" })).await?;
    let stranger = s.add_run_of_another_user().await?;

    // Assert
    expect_that!(over.status, eq(StatusCode::CONFLICT));
    expect_that!(code(&over), some(eq("run_over")));
    expect_that!(stranger.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

impl Scene {
    /// A create in a run that exists, made by someone else.
    async fn add_run_of_another_user(&self) -> Result<TestResponse> {
        let (other, other_heap) = self.app.register("bob@example.test").await?;
        let path = format!("{}/items", self.app.heap_url(&other_heap).await?);
        self.app
            .send_with(
                Method::POST,
                &path,
                Some(&other),
                Some(json!({ "title": "X", "need": "expected", "description": "Context" })),
                &[CSRF, ("upheap-run", &self.run)],
            )
            .await
    }
}

#[gtest]
#[tokio::test]
async fn a_run_adds_steps_to_what_it_added_and_nothing_else_there() -> Result<()> {
    // Arrange
    let s = scene().await?;
    let added = s
        .add(json!({ "title": "Follow up", "kind": "task", "description": "Context" }))
        .await?;
    let key = added.body["key"].as_str().context("key")?.to_owned();
    let path = s.path_of(&s.heap, &key).await?;
    let other = s
        .app
        .new_item(&s.session, &s.heap, json!({ "title": "Other", "kind": "task" }))
        .await?;
    let other_path = s.path_of(&s.heap, other["key"].as_str().context("key")?).await?;

    // Act
    let step = s
        .in_run(
            Method::POST,
            &format!("{path}/steps"),
            Some(json!({ "title": "Do it" })),
        )
        .await?;
    let patch = s.patch_in_run(&path, json!({ "description": "Rewritten" })).await?;
    let comment = s
        .in_run(
            Method::POST,
            &format!("{path}/comments"),
            Some(json!({ "body": "A comment" })),
        )
        .await?;
    let other_step = s
        .in_run(
            Method::POST,
            &format!("{other_path}/steps"),
            Some(json!({ "title": "No" })),
        )
        .await?;

    // Assert
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(step.body["steps"][0]["title"], eq(&json!("Do it")));
    expect_that!(patch.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&patch), some(eq("run_wrong_item")));
    expect_that!(comment.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&comment), some(eq("run_wrong_item")));
    expect_that!(other_step.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&other_step), some(eq("run_wrong_item")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_later_run_on_the_same_item_cant_add_steps_to_what_an_earlier_run_added() -> Result<()> {
    // Arrange: the first run adds a task and fails, so the turn stays open and a second run takes the item.
    let s = scene().await?;
    let added = s
        .add(json!({ "title": "Follow up", "kind": "task", "description": "Context" }))
        .await?;
    anyhow::ensure!(added.status == StatusCode::CREATED, "add failed: {}", added.body);
    let path = s.path_of(&s.heap, added.body["key"].as_str().context("key")?).await?;
    let finished = s
        .app
        .send(
            Method::POST,
            &format!("/api/v1/work/runs/{}/finish", s.run),
            Some(&s.session),
            Some(json!({})),
        )
        .await?;
    anyhow::ensure!(finished.status == StatusCode::OK, "finish failed: {}", finished.body);
    let second = claim(&s.app, &s.session, &s.heap_ref).await?;

    // Act
    let step = s
        .in_run_as(
            &second,
            Method::POST,
            &format!("{path}/steps"),
            Some(json!({ "title": "No" })),
        )
        .await?;

    // Assert
    expect_that!(step.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&step), some(eq("run_wrong_item")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_adds_a_step_to_a_task_it_added_in_another_heap() -> Result<()> {
    // Arrange
    let s = scene().await?;
    let me = s.app.send(Method::GET, "/api/v1/me", Some(&s.session), None).await?;
    let team_slug = me.body["teams"][0]["slug"].as_str().context("slug")?.to_owned();
    let other = s
        .app
        .send(
            Method::POST,
            &format!("/api/v1/teams/{team_slug}/heaps"),
            Some(&s.session),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    anyhow::ensure!(other.status == StatusCode::CREATED, "heap failed: {}", other.body);
    let other_heap = other.body["id"].as_str().context("id")?.to_owned();
    let added = s
        .add_to(
            &other_heap,
            json!({ "title": "Follow up", "kind": "task", "description": "Context" }),
        )
        .await?;
    anyhow::ensure!(added.status == StatusCode::CREATED, "add failed: {}", added.body);
    let path = s
        .path_of(&other_heap, added.body["key"].as_str().context("key")?)
        .await?;

    // Act
    let step = s
        .in_run(
            Method::POST,
            &format!("{path}/steps"),
            Some(json!({ "title": "Do it" })),
        )
        .await?;

    // Assert
    expect_that!(step.status, eq(StatusCode::CREATED));
    expect_that!(step.body["steps"][0]["title"], eq(&json!("Do it")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_step_on_an_item_it_added_in_an_archived_heap_is_refused_as_archived_not_as_run_over() -> Result<()> {
    // Arrange
    let s = scene().await?;
    let me = s.app.send(Method::GET, "/api/v1/me", Some(&s.session), None).await?;
    let team_slug = me.body["teams"][0]["slug"].as_str().context("slug")?.to_owned();
    let other = s
        .app
        .send(
            Method::POST,
            &format!("/api/v1/teams/{team_slug}/heaps"),
            Some(&s.session),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    anyhow::ensure!(other.status == StatusCode::CREATED, "heap failed: {}", other.body);
    let other_heap = other.body["id"].as_str().context("id")?.to_owned();
    let added = s
        .add_to(
            &other_heap,
            json!({ "title": "Follow up", "kind": "task", "description": "Context" }),
        )
        .await?;
    anyhow::ensure!(added.status == StatusCode::CREATED, "add failed: {}", added.body);
    let path = s
        .path_of(&other_heap, added.body["key"].as_str().context("key")?)
        .await?;
    let archive_url = format!("{}/archive", s.app.heap_url(&other_heap).await?);
    let archived = s.app.send(Method::POST, &archive_url, Some(&s.session), None).await?;
    anyhow::ensure!(archived.status.is_success(), "archive failed: {}", archived.body);

    // Act
    let step = s
        .in_run(
            Method::POST,
            &format!("{path}/steps"),
            Some(json!({ "title": "Do it" })),
        )
        .await?;

    // Assert
    expect_that!(code(&step), some(eq("heap_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn every_other_write_outside_the_runs_item_is_still_refused() -> Result<()> {
    // Arrange
    let s = scene().await?;
    let other = s.app.new_item(&s.session, &s.heap, json!({ "title": "Other" })).await?;
    let other_path = s.path_of(&s.heap, other["key"].as_str().context("key")?).await?;

    // Act
    let patch = s
        .patch_in_run(&other_path, json!({ "description": "Not mine" }))
        .await?;
    let tag = s
        .in_run(
            Method::POST,
            &format!("{}/tags", s.app.heap_url(&s.heap).await?),
            Some(json!({ "name": "nope" })),
        )
        .await?;

    // Assert
    expect_that!(patch.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&patch), some(eq("run_wrong_item")));
    expect_that!(tag.status, eq(StatusCode::FORBIDDEN));
    expect_that!(code(&tag), some(eq("run_forbidden")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn outside_a_run_an_item_is_as_written_with_no_origin_and_prepare_opens_a_turn() -> Result<()> {
    // Arrange
    let s = scene().await?;
    set_prepare(&s.app, &s.session, &s.heap).await?;

    // Act
    let added = s
        .app
        .new_item(
            &s.session,
            &s.heap,
            json!({ "title": "By hand", "description": "Context" }),
        )
        .await?;

    // Assert
    expect_that!(added["stage"], eq(&json!("as_written")));
    expect_that!(added["origin"], eq(&Value::Null));
    expect_that!(added["added_in_run"], eq(&json!(false)));
    expect_that!(added["turn"]["source"], eq(&json!("prepare")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_audit_entry_of_an_item_a_run_added_carries_the_run() -> Result<()> {
    // Arrange
    let s = scene().await?;

    // Act
    let added = s.add(json!({ "title": "Follow up", "description": "Context" })).await?;

    // Assert
    let id = added.body["id"].as_str().context("id")?;
    let runs = s.app.audit_runs(id).await?;
    expect_that!(runs, eq(&vec![("item_created".to_owned(), Some(s.run.clone()))]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_heap_audit_names_the_run_and_item_of_an_agents_change_and_not_of_a_persons() -> Result<()> {
    // Arrange
    let s = scene().await?;
    let path = s.path_of(&s.heap, &s.held).await?;

    // Act: the run sets the context, then the person edits the title.
    let by_run = s
        .patch_in_run(&path, json!({ "description": "Context from the run" }))
        .await?;
    let current = s.app.send(Method::GET, &path, Some(&s.session), None).await?;
    let by_person = s
        .app
        .patch(
            &s.session,
            &path,
            &current.body["version"],
            json!({ "title": "Renamed" }),
        )
        .await?;
    let audit = s
        .app
        .send(
            Method::GET,
            &format!(
                "{}/audit?entity_id={}",
                s.app.heap_url(&s.heap).await?,
                current.body["id"].as_str().context("id")?
            ),
            Some(&s.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(by_run.status, eq(StatusCode::OK));
    expect_that!(by_person.status, eq(StatusCode::OK));
    let entries = audit.body["entries"].as_array().context("entries")?;
    let runs: Vec<&Value> = entries.iter().map(|entry| &entry["run"]).collect();
    // Newest first: the person's edit, the run's edit, the creation.
    expect_that!(runs[0], eq(&Value::Null));
    expect_that!(runs[1]["id"], eq(&json!(s.run)));
    expect_that!(runs[1]["item"]["key"], eq(&json!(s.held)));
    expect_that!(runs[1]["item"]["heap"]["id"], eq(&json!(s.heap)));
    expect_that!(runs[1]["item"]["heap"]["slug"].is_string(), eq(true));
    expect_that!(runs[1]["item"]["heap"]["team_slug"].is_string(), eq(true));
    expect_that!(runs[2], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_runs_item_in_a_team_the_reader_is_not_in_is_left_out_of_the_audit_entry() -> Result<()> {
    // Arrange: Ada's run adds a task to a shared team's heap, which Bob belongs to; Bob is not in Ada's personal team.
    let s = scene().await?;
    let (bob, _) = s.app.register_as("bob@example.test", "Bob").await?;
    let (team_id, shared_heap) = s.app.shared_heap(&s.session).await?;
    let me = s.app.send(Method::GET, "/api/v1/me", Some(&bob), None).await?;
    let bob_id = me.body["user"]["id"].as_str().context("id")?;
    s.app.add_member(&team_id, bob_id, Role::Member).await?;
    let added = s
        .add_to(&shared_heap, json!({ "title": "Follow up", "description": "Context" }))
        .await?;
    anyhow::ensure!(added.status == StatusCode::CREATED, "add failed: {}", added.body);

    // Act
    let audit = s
        .app
        .send(
            Method::GET,
            &format!("{}/audit", s.app.heap_url(&shared_heap).await?),
            Some(&bob),
            None,
        )
        .await?;

    // Assert
    expect_that!(audit.status, eq(StatusCode::OK));
    let entry = audit.body["entries"]
        .as_array()
        .context("entries")?
        .iter()
        .find(|entry| entry["action"] == "item_created")
        .context("no item_created entry")?;
    expect_that!(entry["run"]["id"], eq(&json!(s.run)));
    expect_that!(entry["run"]["item"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn someone_who_cannot_see_the_origin_sees_that_an_agent_added_the_item() -> Result<()> {
    // Arrange: Bob's team has a heap; Ada belongs to it and her run adds there. Bob isn't in Ada's team.
    let s = scene().await?;
    let (bob, _) = s.app.register_as("bob@example.test", "Bob").await?;
    let (team_id, bob_heap) = s.app.shared_heap(&bob).await?;
    let me = s.app.send(Method::GET, "/api/v1/me", Some(&s.session), None).await?;
    let ada = me.body["user"]["id"].as_str().context("id")?;
    s.app.add_member(&team_id, ada, Role::Member).await?;

    // Act
    let added = s
        .add_to(&bob_heap, json!({ "title": "Follow up", "description": "Context" }))
        .await?;
    let key = added.body["key"].as_str().context("key")?;
    let seen = s
        .app
        .send(Method::GET, &s.path_of(&bob_heap, key).await?, Some(&bob), None)
        .await?;

    // Assert
    expect_that!(added.status, eq(StatusCode::CREATED));
    expect_that!(added.body["origin"]["key"], eq(&json!(s.held)));
    expect_that!(seen.status, eq(StatusCode::OK));
    expect_that!(seen.body["origin"], eq(&Value::Null));
    expect_that!(seen.body["added_in_run"], eq(&json!(true)));
    Ok(())
}
