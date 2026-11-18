//! Moving an item to another heap, of its team or another: what comes along, what is dropped, the key, the blockers,
//! the history that follows it, and the refusals.

mod common;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{CSRF, TestApp, TestResponse};
use domain::Duration;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};
use service::ScriptedRandom;

/// The random bytes that spell `key`, one per character of Crockford's base32.
fn bytes_of(key: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    key.bytes()
        .filter_map(|c| ALPHABET.iter().position(|a| *a == c))
        .filter_map(|i| u8::try_from(i).ok())
        .collect()
}

fn id(body: &Value) -> String {
    body["id"].as_str().unwrap_or_default().to_owned()
}

/// Makes a heap called `name` in the team of `heap` and returns its ID.
async fn new_heap(app: &TestApp, session: &str, heap: &str, name: &str) -> Result<String> {
    let url = app.heap_url(heap).await?;
    let team = url.split("/heaps/").next().context("heap url")?;
    let created = app
        .send(
            Method::POST,
            &format!("{team}/heaps"),
            Some(session),
            Some(json!({ "name": name })),
        )
        .await?;
    anyhow::ensure!(
        created.status == StatusCode::CREATED,
        "create heap failed: {}",
        created.body
    );
    Ok(id(&created.body))
}

/// Moves the item at `item_url` to `target`, with `version` in `If-Match`.
async fn move_item(
    app: &TestApp,
    session: &str,
    item_url: &str,
    version: &Value,
    target: &str,
) -> Result<TestResponse> {
    app.send_with(
        Method::POST,
        &format!("{item_url}/move"),
        Some(session),
        Some(json!({ "heap_id": target })),
        &[CSRF, ("if-match", &version.to_string())],
    )
    .await
}

/// Moves a fresh read of the item at `item_url`.
async fn move_current(app: &TestApp, session: &str, item_url: &str, target: &str) -> Result<TestResponse> {
    let item = get(app, session, item_url).await?;
    move_item(app, session, item_url, &item.body["version"], target).await
}

async fn get(app: &TestApp, session: &str, url: &str) -> Result<TestResponse> {
    app.send(Method::GET, url, Some(session), None).await
}

async fn edit(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<TestResponse> {
    let current = get(app, session, &app.item_url(id(item)).await?).await?;
    app.patch(session, &app.item_url(id(item)).await?, &current.body["version"], body)
        .await
}

async fn history(app: &TestApp, session: &str, heap: &str, entity: &str) -> Result<Value> {
    let audit = app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={entity}", app.heap_url(heap).await?),
            Some(session),
            None,
        )
        .await?;
    Ok(audit.body)
}

/// The changes of an audit entry.
fn changes(entry: &Value) -> Vec<Value> {
    entry["changes"].as_array().cloned().unwrap_or_default()
}

fn actions(history: &Value) -> Vec<&str> {
    history["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["action"].as_str()).collect())
        .unwrap_or_default()
}

fn allowed(item: &Value) -> Vec<&str> {
    item["allowed_actions"]
        .as_array()
        .map(|actions| actions.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn ranked_ids(list: &Value) -> Vec<String> {
    list["ranked"]
        .as_array()
        .map(|entries| entries.iter().map(|e| id(&e["item"])).collect())
        .unwrap_or_default()
}

/// A user with two heaps in their team, and an open note in the first.
struct World {
    app: TestApp,
    session: String,
    from: String,
    to: String,
    item: Value,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    world_on(app).await
}

async fn world_on(app: TestApp) -> Result<World> {
    let (session, from) = app.register("ada@example.test").await?;
    let to = new_heap(&app, &session, &from, "Agent work").await?;
    let item = app
        .new_item(&session, &from, json!({ "title": "Rotate the certs" }))
        .await?;
    Ok(World {
        app,
        session,
        from,
        to,
        item,
    })
}

impl World {
    async fn item_url(&self) -> Result<String> {
        self.app.item_url(id(&self.item)).await
    }

    /// Moves the item to the second heap, at the version it has now.
    async fn moved(&self) -> Result<TestResponse> {
        move_current(&self.app, &self.session, &self.item_url().await?, &self.to).await
    }

    async fn list(&self, heap: &str) -> Result<Value> {
        let list = get(
            &self.app,
            &self.session,
            &format!("{}/items", self.app.heap_url(heap).await?),
        )
        .await?;
        Ok(list.body)
    }
}

#[gtest]
#[tokio::test]
async fn an_item_arrives_with_its_context_steps_questions_comments_and_turn() -> Result<()> {
    // Arrange
    let w = world().await?;
    let old_url = w.item_url().await?;
    let made = edit(
        &w.app,
        &w.session,
        &w.item,
        json!({ "description": "Use the new CA.", "kind": "task" }),
    )
    .await?;
    anyhow::ensure!(made.status == StatusCode::OK, "edit failed: {}", made.body);
    for title in ["Inventory", "Roll out"] {
        w.app
            .send(
                Method::POST,
                &format!("{old_url}/steps"),
                Some(&w.session),
                Some(json!({ "title": title })),
            )
            .await?;
    }
    let ticked = w
        .app
        .patch(
            &w.session,
            &format!("{old_url}/steps/1"),
            &json!(1),
            json!({ "state": "ticked" }),
        )
        .await?;
    anyhow::ensure!(ticked.status == StatusCode::OK, "tick failed: {}", ticked.body);
    for text in ["Which CA?", "Which stack?"] {
        w.app
            .send(
                Method::POST,
                &format!("{old_url}/questions"),
                Some(&w.session),
                Some(json!({ "kind": "question", "text": text })),
            )
            .await?;
    }
    let answered = w
        .app
        .patch(
            &w.session,
            &format!("{old_url}/questions/1"),
            &json!(1),
            json!({ "reply": { "comment": "Internal" } }),
        )
        .await?;
    anyhow::ensure!(answered.status == StatusCode::OK, "reply failed: {}", answered.body);
    for body in ["Use the internal CA.", "Done in staging."] {
        let commented = w
            .app
            .send(
                Method::POST,
                &format!("{old_url}/comments"),
                Some(&w.session),
                Some(json!({ "body": body })),
            )
            .await?;
        anyhow::ensure!(
            commented.status == StatusCode::CREATED,
            "comment failed: {}",
            commented.body
        );
    }
    let sent = w
        .app
        .send(
            Method::POST,
            &format!("{old_url}/turn"),
            Some(&w.session),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
    let before = get(&w.app, &w.session, &old_url).await?.body;

    // Act
    let moved = move_item(&w.app, &w.session, &old_url, &before["version"], &w.to).await?;
    let new_url = w.item_url().await?;
    let there = get(&w.app, &w.session, &new_url).await?;
    let old = get(&w.app, &w.session, &old_url).await?;
    let (source_list, target_list) = (w.list(&w.from).await?, w.list(&w.to).await?);

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body, eq(&there.body));
    expect_that!(there.body["id"], eq(&before["id"]));
    expect_that!(there.body["key"], eq(&before["key"]));
    expect_that!(there.body["heap_id"], eq(&json!(w.to)));
    expect_that!(
        there.body["version"],
        eq(&json!(before["version"].as_u64().unwrap_or(0) + 1))
    );
    expect_that!(there.body["description"], eq(&json!("Use the new CA.")));
    expect_that!(there.body["kind"], eq(&json!("task")));
    expect_that!(there.body["steps"], eq(&before["steps"]));
    expect_that!(there.body["step_progress"], eq(&json!({ "ticked": 1, "total": 2 })));
    expect_that!(there.body["questions"], eq(&before["questions"]));
    expect_that!(there.body["comments"], eq(&before["comments"]));
    expect_that!(there.body["comments"].as_array().map(Vec::len), some(eq(2)));
    expect_that!(there.body["turn"], eq(&before["turn"]));
    expect_that!(there.body["turn"].is_object(), eq(true));
    expect_that!(there.body["waits_for_you"], eq(&before["waits_for_you"]));
    expect_that!(old.status, eq(StatusCode::OK));
    expect_that!(id(&old.body), eq(&id(&before)));
    expect_that!(old.body["heap_id"], eq(&json!(w.to)));
    expect_that!(ranked_ids(&target_list), elements_are![eq(&id(&before))]);
    expect_that!(ranked_ids(&source_list), is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_open_turn_stays_open_and_the_target_heaps_queue_offers_the_item() -> Result<()> {
    // Arrange
    let w = world().await?;
    let old_url = w.item_url().await?;
    w.app
        .send(
            Method::POST,
            &format!("{old_url}/turn"),
            Some(&w.session),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    let target_url = w.app.heap_url(&w.to).await?;
    let target_ref = target_url
        .strip_prefix("/api/v1/teams/")
        .context("heap url")?
        .replace("/heaps/", "/");

    // Act
    let moved = w.moved().await?;
    let next = w
        .app
        .send(
            Method::GET,
            &format!("/api/v1/work/next?heaps={target_ref}"),
            Some(&w.session),
            None,
        )
        .await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["turn"]["goal"], eq(&json!("with_context")));
    expect_that!(next.body["item"]["key"], eq(&w.item["key"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_history_follows_the_item_to_the_new_heap() -> Result<()> {
    // Arrange
    let w = world().await?;
    let infra = w.app.new_tag_id(&w.session, &w.from, "infra").await?;
    w.app.clock.advance(Duration::from_millis(1));
    edit(
        &w.app,
        &w.session,
        &w.item,
        json!({ "tag_ids": [infra], "title": "Rotate all certs" }),
    )
    .await?;
    w.app.clock.advance(Duration::from_millis(1));

    // Act
    let moved = w.moved().await?;
    w.app.clock.advance(Duration::from_millis(1));
    edit(&w.app, &w.session, &w.item, json!({ "title": "Rotate all the certs" })).await?;
    let log = history(&w.app, &w.session, &w.to, &id(&w.item)).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(
        actions(&log),
        elements_are![
            eq(&"item_updated"),
            eq(&"item_moved"),
            eq(&"item_updated"),
            eq(&"item_created")
        ]
    );
    let pre_move_edit = &log["entries"][2];
    expect_that!(
        changes(pre_move_edit),
        contains(eq(
            &json!({ "field": "tags", "label": "Tags", "before": null, "after": "infra", "text_diff": null })
        ))
    );
    let move_entry = &log["entries"][1];
    expect_that!(move_entry["phrase"], eq(&json!("moved item")));
    expect_that!(move_entry["entity"]["label"], eq(&json!("Rotate all the certs")));
    expect_that!(
        changes(move_entry),
        contains(eq(
            &json!({ "field": "heap", "label": "Heap", "before": "Personal", "after": "Agent work", "text_diff": null })
        ))
    );
    expect_that!(
        changes(move_entry),
        contains(eq(
            &json!({ "field": "tags", "label": "Tags", "before": "infra", "after": null, "text_diff": null })
        ))
    );
    // The entry is recorded in the heap the item went to, and the old heap's own log doesn't have it.
    let old_heap_log = w
        .app
        .send(
            Method::GET,
            &format!("{}/audit", w.app.heap_url(&w.from).await?),
            Some(&w.session),
            None,
        )
        .await?;
    expect_that!(actions(&old_heap_log.body), not(contains(eq(&"item_moved"))));
    Ok(())
}

/// The heap's own log, without `entity_id`.
async fn heap_log(app: &TestApp, session: &str, heap: &str) -> Result<Value> {
    let audit = get(app, session, &format!("{}/audit", app.heap_url(heap).await?)).await?;
    Ok(audit.body)
}

#[gtest]
#[tokio::test]
async fn the_plain_heap_logs_name_what_moved_between_the_heaps() -> Result<()> {
    // Arrange
    let w = world().await?;
    let infra = w.app.new_tag_id(&w.session, &w.from, "infra").await?;
    edit(&w.app, &w.session, &w.item, json!({ "tag_ids": [infra] })).await?;
    w.app.clock.advance(Duration::from_millis(1));

    // Act
    let moved = w.moved().await?;
    let target_log = heap_log(&w.app, &w.session, &w.to).await?;
    let source_log = heap_log(&w.app, &w.session, &w.from).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    let move_entry = &target_log["entries"][0];
    expect_that!(move_entry["action"], eq(&json!("item_moved")));
    expect_that!(
        changes(move_entry),
        contains(eq(
            &json!({ "field": "heap", "label": "Heap", "before": "Personal", "after": "Agent work", "text_diff": null })
        ))
    );
    expect_that!(
        changes(move_entry),
        contains(eq(
            &json!({ "field": "tags", "label": "Tags", "before": "infra", "after": null, "text_diff": null })
        ))
    );
    let item_entries: Vec<&Value> = source_log["entries"]
        .as_array()
        .context("entries")?
        .iter()
        .filter(|entry| entry["entity"]["type"] == "item")
        .collect();
    expect_that!(item_entries.len(), ge(2));
    for entry in item_entries {
        expect_that!(entry["entity"]["label"], eq(&json!("Rotate the certs")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_key_the_target_heap_has_is_replaced_by_a_new_one() -> Result<()> {
    // Arrange: both items draw k3x9a, one in each heap. The moved item then draws m4n5p.
    let random = Arc::new(ScriptedRandom::default());
    let app = TestApp::start_with_random(random.clone()).await?;
    let (session, from) = app.register("ada@example.test").await?;
    let to = new_heap(&app, &session, &from, "Agent work").await?;
    for key in ["k3x9a", "k3x9a", "m4n5p"] {
        random.push(bytes_of(key));
    }
    let item = app.new_item(&session, &from, json!({ "title": "Mine" })).await?;
    let squatter = app.new_item(&session, &to, json!({ "title": "Theirs" })).await?;
    let url = app.item_url(id(&item)).await?;

    // Act
    let moved = move_item(&app, &session, &url, &item["version"], &to).await?;
    let audit = app.audit_entries(&id(&item)).await?;

    // Assert
    expect_that!(item["key"], eq(&json!("k3x9a")));
    expect_that!(squatter["key"], eq(&json!("k3x9a")));
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["key"], eq(&json!("m4n5p")));
    let (action, diff) = audit.last().context("no entries")?;
    expect_that!(action, eq("item_moved"));
    expect_that!(diff["key"], eq(&json!({ "before": "k3x9a", "after": "m4n5p" })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_free_key_is_kept() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let moved = w.moved().await?;
    let audit = w.app.audit_entries(&id(&w.item)).await?;

    // Assert
    expect_that!(moved.body["key"], eq(&w.item["key"]));
    let (_, diff) = audit.last().context("no entries")?;
    expect_that!(diff.get("key"), none());
    expect_that!(diff["heap"], eq(&json!({ "before": w.from, "after": w.to })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn team_tags_come_along_with_the_heap_tags_that_both_heaps_have() -> Result<()> {
    // Arrange
    let w = world().await?;
    let team_path = w.app.heap_url(&w.from).await?;
    let team_path = team_path.split("/heaps/").next().context("heap url")?;
    let made = w
        .app
        .send(
            Method::POST,
            &format!("{team_path}/tags"),
            Some(&w.session),
            Some(json!({ "name": "shared" })),
        )
        .await?;
    let team_tag = id(&made.body);
    let source_infra = w.app.new_tag_id(&w.session, &w.from, "infra").await?;
    let target_infra = w.app.new_tag_id(&w.session, &w.to, "Infra").await?;
    edit(
        &w.app,
        &w.session,
        &w.item,
        json!({ "tag_ids": [team_tag, source_infra] }),
    )
    .await?;

    // Act
    let moved = w.moved().await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    let kept: Vec<&Value> = moved.body["tags"]
        .as_array()
        .context("no tags")?
        .iter()
        .map(|t| &t["id"])
        .collect();
    expect_that!(
        kept,
        unordered_elements_are![eq(&&json!(team_tag)), eq(&&json!(target_infra))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tags_are_kept_by_name_ignoring_case_and_the_rest_are_dropped() -> Result<()> {
    // Arrange
    let w = world().await?;
    let source_infra = w.app.new_tag_id(&w.session, &w.from, "Infra").await?;
    let later = w.app.new_tag_id(&w.session, &w.from, "later").await?;
    let target_infra = w.app.new_tag_id(&w.session, &w.to, "infra").await?;
    edit(&w.app, &w.session, &w.item, json!({ "tag_ids": [source_infra, later] })).await?;

    // Act
    let moved = w.moved().await?;
    let audit = w.app.audit_entries(&id(&w.item)).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["tags"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(moved.body["tags"][0]["id"], eq(&json!(target_infra)));
    let (_, diff) = audit.last().context("no entries")?;
    let mut before: Vec<String> = serde_json::from_value(diff["tags"]["before"].clone())?;
    before.sort();
    let mut expected = vec![source_infra, later];
    expected.sort();
    expect_that!(before, eq(&expected));
    expect_that!(diff["tags"]["after"], eq(&json!([target_infra])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn values_are_kept_by_parameter_name_and_option_label_and_the_rest_are_dropped() -> Result<()> {
    // Arrange
    let w = world().await?;
    let make = |heap: String, name: &'static str, options: Vec<&'static str>| {
        let app = &w.app;
        let session = &w.session;
        async move {
            let created = app
                .send(
                    Method::POST,
                    &format!("{}/parameters", app.heap_url(&heap).await?),
                    Some(session),
                    Some(json!({ "name": name, "options": options })),
                )
                .await?;
            anyhow::ensure!(
                created.status == StatusCode::CREATED,
                "parameter failed: {}",
                created.body
            );
            Ok::<Value, anyhow::Error>(created.body)
        }
    };
    let option = |parameter: &Value, label: &str| -> String {
        parameter["options"]
            .as_array()
            .and_then(|options| options.iter().find(|o| o["label"] == label))
            .map(id)
            .unwrap_or_default()
    };
    let area = make(w.from.clone(), "Area", vec!["Backend", "Web"]).await?;
    let customer = make(w.from.clone(), "Customer", vec!["Acme"]).await?;
    let region = make(w.from.clone(), "Region", vec!["EU"]).await?;
    let target_area = make(w.to.clone(), "area", vec!["backend", "Mobile"]).await?;
    make(w.to.clone(), "Customer", vec!["Globex"]).await?;
    let set = edit(
        &w.app,
        &w.session,
        &w.item,
        json!({ "parameter_values": {
            id(&area): option(&area, "Backend"),
            id(&customer): option(&customer, "Acme"),
            id(&region): option(&region, "EU"),
        } }),
    )
    .await?;
    anyhow::ensure!(set.status == StatusCode::OK, "set values failed: {}", set.body);

    // Act
    let moved = w.moved().await?;
    let audit = w.app.audit_entries(&id(&w.item)).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    let values: Vec<(&str, Option<&str>)> = moved.body["parameters"]
        .as_array()
        .context("no parameters")?
        .iter()
        .map(|p| (p["name"].as_str().unwrap_or_default(), p["option"]["label"].as_str()))
        .collect();
    expect_that!(
        values,
        unordered_elements_are![eq(&("area", Some("backend"))), eq(&("Customer", None))]
    );
    expect_that!(
        moved.body["parameters"][0]["option"]["id"],
        eq(&json!(option(&target_area, "backend")))
    );
    let (_, diff) = audit.last().context("no entries")?;
    expect_that!(
        diff["parameters"]["after"],
        eq(&json!({ id(&target_area): option(&target_area, "backend") }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_left_behind_keep_the_moved_blocker_and_wait_for_the_person() -> Result<()> {
    // Arrange: M waits for B, and A waits for M.
    let w = world().await?;
    let b = w.app.new_item(&w.session, &w.from, json!({ "title": "B" })).await?;
    let a = w.app.new_item(&w.session, &w.from, json!({ "title": "A" })).await?;
    let blocked = edit(
        &w.app,
        &w.session,
        &w.item,
        json!({ "state": "blocked", "blocked_by_item_id": id(&b), "blocked_reason": "Waits for the vendor" }),
    )
    .await?;
    anyhow::ensure!(blocked.status == StatusCode::OK, "block M failed: {}", blocked.body);
    let blocked = edit(
        &w.app,
        &w.session,
        &a,
        json!({ "state": "blocked", "blocked_by_item_id": id(&w.item) }),
    )
    .await?;
    anyhow::ensure!(blocked.status == StatusCode::OK, "block A failed: {}", blocked.body);
    w.app.clock.advance(Duration::from_millis(1));

    // Act
    let moved = w.moved().await?;
    let a_after = get(&w.app, &w.session, &w.app.item_url(id(&a)).await?).await?;
    let a_log = history(&w.app, &w.session, &w.from, &id(&a)).await?;
    let waiting = get(&w.app, &w.session, "/api/v1/me/waiting").await?;

    // Assert: M loses its own blocker in the heap it left; A keeps M, now gone, and waits for the person.
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["state"], eq(&json!("blocked")));
    expect_that!(moved.body["blocked_by"], eq(&Value::Null));
    // Its reason is about the outside world, so it moves along.
    expect_that!(moved.body["blocked_reason"], eq(&json!("Waits for the vendor")));
    expect_that!(a_after.body["state"], eq(&json!("blocked")));
    expect_that!(
        a_after.body["blocked_by"],
        eq(
            &json!({ "id": id(&w.item), "key": moved.body["key"], "title": "Rotate the certs", "kind": "note", "state": "blocked" })
        )
    );
    expect_that!(a_after.body["blocker_gone"]["how"], eq(&json!("moved")));
    expect_that!(a_after.body["blocker_gone"]["moved_to"]["id"], eq(&json!(w.to)));
    expect_that!(
        a_after.body["blocker_gone"]["moved_to"]["name"],
        eq(&json!("Agent work"))
    );
    expect_that!(a_after.body["waits_for_you"], eq(&json!(true)));
    // Nothing was written to A: its blocker went, and A stayed as it was.
    expect_that!(a_after.body["version"], eq(&json!(2)));
    expect_that!(actions(&a_log), elements_are![eq(&"item_updated"), eq(&"item_created")]);
    expect_that!(waiting.body["count"], eq(&json!(1)));
    expect_that!(waiting.body["rows"][0]["kind"], eq(&json!("blocker_gone")));
    expect_that!(waiting.body["rows"][0]["item"]["id"], eq(&json!(id(&a))));
    expect_that!(
        waiting.body["rows"][0]["blocker"]["item"]["id"],
        eq(&json!(id(&w.item)))
    );
    expect_that!(waiting.body["rows"][0]["blocker"]["gone"]["how"], eq(&json!("moved")));
    expect_that!(
        waiting.body["rows"][0]["blocker"]["allowed_actions"],
        eq(&json!(["change_state"]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blocker_moved_back_holds_what_it_left_again() -> Result<()> {
    // Arrange: A waits for M, which is moved away.
    let w = world().await?;
    let a = w.app.new_item(&w.session, &w.from, json!({ "title": "A" })).await?;
    let blocked = edit(
        &w.app,
        &w.session,
        &a,
        json!({ "state": "blocked", "blocked_by_item_id": id(&w.item) }),
    )
    .await?;
    anyhow::ensure!(blocked.status == StatusCode::OK, "block A failed: {}", blocked.body);
    let moved = w.moved().await?;
    anyhow::ensure!(moved.status == StatusCode::OK, "move failed: {}", moved.body);

    // Act
    let back = move_current(&w.app, &w.session, &w.app.item_url(id(&w.item)).await?, &w.from).await?;
    let a_after = get(&w.app, &w.session, &w.app.item_url(id(&a)).await?).await?;
    let waiting = get(&w.app, &w.session, "/api/v1/me/waiting").await?;

    // Assert
    expect_that!(back.status, eq(StatusCode::OK));
    expect_that!(a_after.body["blocked_by"]["id"], eq(&json!(id(&w.item))));
    expect_that!(a_after.body["blocker_gone"], eq(&Value::Null));
    expect_that!(a_after.body["waits_for_you"], eq(&json!(false)));
    expect_that!(waiting.body["count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn moving_to_the_heap_it_is_in_changes_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;
    let url = w.item_url().await?;

    // Act
    let same = move_item(&w.app, &w.session, &url, &w.item["version"], &w.from).await?;
    let stale = move_item(&w.app, &w.session, &url, &json!(7), &w.from).await?;
    let audit = w.app.audit_entries(&id(&w.item)).await?;

    // Assert
    expect_that!(same.status, eq(StatusCode::OK));
    expect_that!(same.body["version"], eq(&w.item["version"]));
    expect_that!(same.body["heap_id"], eq(&json!(w.from)));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(audit.len(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_missing_heap_and_a_heap_of_a_team_the_caller_is_not_in_are_not_found() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (grace, grace_heap) = w.app.register("grace@example.test").await?;
    let url = w.item_url().await?;

    // Act
    let nowhere = move_item(
        &w.app,
        &w.session,
        &url,
        &w.item["version"],
        "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40",
    )
    .await?;
    let strangers = move_item(&w.app, &w.session, &url, &w.item["version"], &grace_heap).await?;
    let outsider = move_item(&w.app, &grace, &url, &w.item["version"], &grace_heap).await?;

    // Assert
    for refused in [&nowhere, &strangers] {
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(refused.body["code"], eq(&json!("heap_not_in_team")));
        expect_that!(refused.body["field"], eq(&json!("heap_id")));
    }
    expect_that!(outsider.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_on_either_side_refuses_the_move() -> Result<()> {
    // Arrange
    let w = world().await?;
    let url = w.item_url().await?;

    // Act
    w.app.set_heap_archived(&w.to, true).await?;
    let archived_target = move_item(&w.app, &w.session, &url, &w.item["version"], &w.to).await?;
    w.app.set_heap_archived(&w.to, false).await?;
    w.app.set_heap_archived(&w.from, true).await?;
    let archived_source = move_item(&w.app, &w.session, &url, &w.item["version"], &w.to).await?;
    let read = get(&w.app, &w.session, &url).await?;

    // Assert
    for refused in [&archived_target, &archived_source] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(refused.body["code"], eq(&json!("heap_archived")));
    }
    expect_that!(allowed(&read.body), is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_cannot_move() -> Result<()> {
    // Arrange
    let w = world().await?;
    let closed = edit(&w.app, &w.session, &w.item, json!({ "state": "done" })).await?;
    let url = w.item_url().await?;

    // Act
    let refused = move_item(&w.app, &w.session, &url, &closed.body["version"], &w.to).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("item_closed")));
    expect_that!(allowed(&closed.body), not(contains(eq(&"move"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_version_unknown_fields_and_the_header_are_checked() -> Result<()> {
    // Arrange
    let w = world().await?;
    let url = w.item_url().await?;
    edit(&w.app, &w.session, &w.item, json!({ "title": "Changed meanwhile" })).await?;

    // Act
    let stale = move_item(&w.app, &w.session, &url, &w.item["version"], &w.to).await?;
    let missing = w
        .app
        .send(
            Method::POST,
            &format!("{url}/move"),
            Some(&w.session),
            Some(json!({ "heap_id": w.to })),
        )
        .await?;
    let unknown = w
        .app
        .send_with(
            Method::POST,
            &format!("{url}/move"),
            Some(&w.session),
            Some(json!({ "heap_id": w.to, "key": "abcde" })),
            &[CSRF, ("if-match", "2")],
        )
        .await?;

    // Assert
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(missing.status, eq(StatusCode::PRECONDITION_REQUIRED));
    expect_that!(missing.body["code"], eq(&json!("version_required")));
    expect_that!(unknown.status, eq(StatusCode::BAD_REQUEST));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_run_may_not_move_an_item() -> Result<()> {
    // Arrange
    let w = world().await?;
    let url = w.item_url().await?;

    // Act
    let refused = w
        .app
        .send_with(
            Method::POST,
            &format!("{url}/move"),
            Some(&w.session),
            Some(json!({ "heap_id": w.to })),
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

#[gtest]
#[tokio::test]
async fn move_is_offered_on_open_items_in_active_heaps() -> Result<()> {
    // Arrange
    let w = world().await?;

    // Act
    let open = get(&w.app, &w.session, &w.item_url().await?).await?;
    let closed = edit(&w.app, &w.session, &w.item, json!({ "state": "done" })).await?;

    // Assert
    expect_that!(allowed(&open.body), contains(eq(&"move")));
    expect_that!(allowed(&closed.body), not(contains(eq(&"move"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_of_the_team_may_move_items_between_its_heaps() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (_, shared) = w.app.shared_heap(&w.session).await?;
    let second = new_heap(&w.app, &w.session, &shared, "Second").await?;
    let item = w
        .app
        .new_item(&w.session, &shared, json!({ "title": "Shared work" }))
        .await?;

    // Act
    let moved = move_item(
        &w.app,
        &w.session,
        &w.app.item_url(id(&item)).await?,
        &item["version"],
        &second,
    )
    .await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["heap_id"], eq(&json!(second)));
    Ok(())
}

/// Ada, an admin of two teams, with a heap in each; Bob, a member of the first only; Grace, a member of the second
/// only; and an open note of Ada's in the first heap.
struct Across {
    app: TestApp,
    ada: String,
    bob: String,
    grace: String,
    from: String,
    to: String,
    item: Value,
}

async fn across() -> Result<Across> {
    let app = TestApp::start().await?;
    let (ada, _) = app.register("ada@example.test").await?;
    let (bob, _) = app.register("bob@example.test").await?;
    let (grace, _) = app.register("grace@example.test").await?;
    let (team_a, from) = app.shared_heap(&ada).await?;
    let (team_b, to) = app.shared_heap(&ada).await?;
    app.add_member(&team_a, &user_id(&app, &bob).await?, Role::Member)
        .await?;
    app.add_member(&team_b, &user_id(&app, &grace).await?, Role::Member)
        .await?;
    let item = app
        .new_item(&ada, &from, json!({ "title": "Rotate the certs" }))
        .await?;
    Ok(Across {
        app,
        ada,
        bob,
        grace,
        from,
        to,
        item,
    })
}

async fn user_id(app: &TestApp, session: &str) -> Result<String> {
    let me = get(app, session, "/api/v1/me").await?;
    Ok(me.body["user"]["id"].as_str().context("no user")?.to_owned())
}

impl Across {
    async fn item_url(&self) -> Result<String> {
        self.app.item_url(id(&self.item)).await
    }

    /// Ada moves the item to the second team's heap.
    async fn moved(&self) -> Result<TestResponse> {
        move_current(&self.app, &self.ada, &self.item_url().await?, &self.to).await
    }
}

#[gtest]
#[tokio::test]
async fn the_old_path_of_a_replaced_key_leads_to_the_moved_item() -> Result<()> {
    // Arrange: as above, the moved item takes m4n5p in the target heap.
    let random = Arc::new(ScriptedRandom::default());
    let app = TestApp::start_with_random(random.clone()).await?;
    let (session, from) = app.register("ada@example.test").await?;
    let to = new_heap(&app, &session, &from, "Agent work").await?;
    for key in ["k3x9a", "k3x9a", "m4n5p"] {
        random.push(bytes_of(key));
    }
    let item = app.new_item(&session, &from, json!({ "title": "Mine" })).await?;
    app.new_item(&session, &to, json!({ "title": "Theirs" })).await?;
    let url = app.item_url(id(&item)).await?;

    // Act
    move_item(&app, &session, &url, &item["version"], &to).await?;
    let old = get(&app, &session, &url).await?;

    // Assert
    expect_that!(old.status, eq(StatusCode::OK));
    expect_that!(id(&old.body), eq(&id(&item)));
    expect_that!(old.body["key"], eq(&json!("m4n5p")));
    expect_that!(old.body["heap_id"], eq(&json!(to)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_moves_to_a_heap_of_another_team_and_each_team_logs_its_side() -> Result<()> {
    // Arrange
    let w = across().await?;
    w.app.clock.advance(Duration::from_millis(1));

    // Act
    let moved = w.moved().await?;
    let heaps = w.app.audit_heaps(&id(&w.item)).await?;
    let entries = w.app.audit_entries(&id(&w.item)).await?;

    // Assert
    expect_that!(moved.status, eq(StatusCode::OK));
    expect_that!(moved.body["heap_id"], eq(&json!(w.to)));
    expect_that!(
        heaps,
        elements_are![
            eq(&("item_created".to_owned(), Some(w.from.clone()))),
            eq(&("item_moved".to_owned(), Some(w.from.clone()))),
            eq(&("item_moved".to_owned(), Some(w.to.clone()))),
        ]
    );
    let (left, arrived) = (&entries[1].1, &entries[2].1);
    expect_that!(
        left,
        eq(&json!({ "heap": { "before": w.from, "after": null }, "item_title": "Rotate the certs" }))
    );
    expect_that!(left.to_string(), not(contains_substring(w.to.as_str())));
    expect_that!(arrived, eq(&json!({ "heap": { "before": null, "after": w.to } })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn history_reads_each_teams_part_by_who_the_reader_is() -> Result<()> {
    // Arrange
    let w = across().await?;
    w.app.clock.advance(Duration::from_millis(1));
    let moved = w.moved().await?;
    anyhow::ensure!(moved.status == StatusCode::OK, "move failed: {}", moved.body);
    let entity = id(&w.item);

    // Act
    let both = history(&w.app, &w.ada, &w.to, &entity).await?;
    let new_team = history(&w.app, &w.grace, &w.to, &entity).await?;
    let old_team = history(&w.app, &w.bob, &w.from, &entity).await?;
    let old_log = get(&w.app, &w.bob, &format!("{}/audit", w.app.heap_url(&w.from).await?)).await?;

    // Assert
    expect_that!(
        actions(&both),
        elements_are![eq(&"item_moved"), eq(&"item_moved"), eq(&"item_created")]
    );
    expect_that!(
        both["entries"][0]["phrase"],
        eq(&json!("moved item here from another team"))
    );
    expect_that!(both["entries"][1]["phrase"], eq(&json!("moved item to another team")));
    expect_that!(both["entries"][0]["changes"][0]["before"], eq(&Value::Null));
    expect_that!(both["entries"][0]["changes"][0]["after"], eq(&json!("Shared")));
    expect_that!(both["entries"][1]["changes"][0]["after"], eq(&Value::Null));
    expect_that!(actions(&new_team), elements_are![eq(&"item_moved")]);
    expect_that!(
        new_team["entries"][0]["entity"]["label"],
        eq(&json!("Rotate the certs"))
    );
    expect_that!(
        actions(&old_team),
        elements_are![eq(&"item_moved"), eq(&"item_created")]
    );
    expect_that!(
        old_team["entries"][0]["entity"]["label"],
        eq(&json!("Rotate the certs"))
    );
    expect_that!(
        old_team["entries"][0]["phrase"],
        eq(&json!("moved item to another team"))
    );
    expect_that!(
        old_log.body["entries"][0]["entity"]["label"],
        eq(&json!("Rotate the certs"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_turn_stays_when_its_person_may_edit_in_the_target_team_and_is_taken_back_otherwise() -> Result<()> {
    // Arrange: Ada's turn on one item, Bob's, who is not in the second team, on another.
    let w = across().await?;
    let bobs = w.app.new_item(&w.ada, &w.from, json!({ "title": "Bob's" })).await?;
    for (session, item) in [(&w.ada, &w.item), (&w.bob, &bobs)] {
        let sent = w
            .app
            .send(
                Method::POST,
                &format!("{}/turn", w.app.item_url(id(item)).await?),
                Some(session),
                Some(json!({ "reach": "one" })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
    }

    // Act
    let adas_moved = w.moved().await?;
    let bobs_moved = move_current(&w.app, &w.ada, &w.app.item_url(id(&bobs)).await?, &w.to).await?;
    let log = w.app.audit_entries(&id(&bobs)).await?;
    let heaps = w.app.audit_heaps(&id(&bobs)).await?;

    // Assert
    expect_that!(adas_moved.body["turn"].is_object(), eq(true));
    expect_that!(bobs_moved.status, eq(StatusCode::OK));
    expect_that!(bobs_moved.body["turn"], eq(&Value::Null));
    let taken_back = log.iter().position(|(action, _)| action == "turn_taken_back");
    expect_that!(taken_back, some(anything()));
    let at = taken_back.unwrap_or_default();
    expect_that!(log[at].1["why"], eq(&json!("moved")));
    // Recorded in the heap the turn belonged to, before the item left it.
    expect_that!(heaps[at].1, eq(&Some(w.from.clone())));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_with_a_heap_made_from_it_stays_in_its_team() -> Result<()> {
    // Arrange: a heap is made from the note.
    let w = across().await?;
    let url = w.app.heap_url(&w.from).await?;
    let url = url.split("/heaps/").next().context("heap url")?.to_owned();
    let made = w
        .app
        .send(
            Method::POST,
            &format!("{url}/heaps"),
            Some(&w.ada),
            Some(json!({ "name": "Blog move", "origin_id": w.item["id"] })),
        )
        .await?;
    anyhow::ensure!(made.status == StatusCode::CREATED, "make heap failed: {}", made.body);
    let same_team = new_heap(&w.app, &w.ada, &w.from, "Second").await?;

    // Act
    let across = w.moved().await?;
    let within = move_current(&w.app, &w.ada, &w.item_url().await?, &same_team).await?;

    // Assert
    expect_that!(across.status, eq(StatusCode::CONFLICT));
    expect_that!(across.body["code"], eq(&json!("note_has_heaps")));
    expect_that!(within.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_note_with_an_archived_heap_made_from_it_stays_in_its_team() -> Result<()> {
    // Arrange: a heap is made from the note, then archived.
    let w = across().await?;
    let url = w.app.heap_url(&w.from).await?;
    let url = url.split("/heaps/").next().context("heap url")?.to_owned();
    let made = w
        .app
        .send(
            Method::POST,
            &format!("{url}/heaps"),
            Some(&w.ada),
            Some(json!({ "name": "Blog move", "origin_id": w.item["id"] })),
        )
        .await?;
    anyhow::ensure!(made.status == StatusCode::CREATED, "make heap failed: {}", made.body);
    w.app
        .set_heap_archived(made.body["id"].as_str().context("heap id")?, true)
        .await?;

    // Act
    let across = w.moved().await?;

    // Assert
    expect_that!(across.status, eq(StatusCode::CONFLICT));
    expect_that!(across.body["code"], eq(&json!("note_has_heaps")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_that_waited_for_the_moved_one_never_names_it() -> Result<()> {
    // Arrange: A waits for the item in the first team.
    let w = across().await?;
    let a = w.app.new_item(&w.ada, &w.from, json!({ "title": "A" })).await?;
    let blocked = edit(
        &w.app,
        &w.ada,
        &a,
        json!({ "state": "blocked", "blocked_by_item_id": id(&w.item) }),
    )
    .await?;
    anyhow::ensure!(blocked.status == StatusCode::OK, "block A failed: {}", blocked.body);
    let moved = w.moved().await?;
    anyhow::ensure!(moved.status == StatusCode::OK, "move failed: {}", moved.body);

    // Act
    let single = get(&w.app, &w.ada, &w.app.item_url(id(&a)).await?).await?;
    let list = get(
        &w.app,
        &w.bob,
        &format!("{}/items?blocked=shown", w.app.heap_url(&w.from).await?),
    )
    .await?;
    let waiting = get(&w.app, &w.ada, "/api/v1/me/waiting").await?;

    // Assert
    let gone = json!({ "how": "moved", "moved_to": null });
    expect_that!(single.body["blocked_by"], eq(&Value::Null));
    expect_that!(single.body["blocker_gone"], eq(&gone));
    expect_that!(single.body["waits_for_you"], eq(&json!(true)));
    let listed = list.body["blocked"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["item"]["id"] == json!(id(&a))))
        .with_context(|| format!("A is not listed: {}", list.body))?;
    expect_that!(listed["item"]["blocked_by"], eq(&Value::Null));
    expect_that!(listed["item"]["blocker_gone"], eq(&gone));
    expect_that!(waiting.body["count"], eq(&json!(1)));
    expect_that!(waiting.body["rows"][0]["kind"], eq(&json!("blocker_left_team")));
    expect_that!(waiting.body["rows"][0]["blocker"], eq(&Value::Null));
    expect_that!(
        waiting.body["rows"][0]["left_team"]["allowed_actions"],
        eq(&json!(["change_state"]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_made_from_a_note_that_moved_away_shows_only_to_readers_of_its_team() -> Result<()> {
    // Arrange: a task is split from the note, and the task moves to the second team.
    let w = across().await?;
    let task = w
        .app
        .new_item(
            &w.ada,
            &w.from,
            json!({ "title": "Task", "kind": "task", "origin": { "item_id": w.item["id"], "age": "keep" } }),
        )
        .await?;
    let moved = move_current(&w.app, &w.ada, &w.app.item_url(id(&task)).await?, &w.to).await?;
    anyhow::ensure!(moved.status == StatusCode::OK, "move failed: {}", moved.body);

    // Act
    let url = w.item_url().await?;
    let by_ada = get(&w.app, &w.ada, &url).await?;
    let by_bob = get(&w.app, &w.bob, &url).await?;

    // Assert
    expect_that!(by_ada.body["became"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(by_bob.body["became"].as_array().map(Vec::len), some(eq(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_key_a_new_item_draws_later_leads_to_the_new_item() -> Result<()> {
    // Arrange: the moved item had k3x9a in the source heap, and a new item draws it there.
    let random = Arc::new(ScriptedRandom::default());
    let app = TestApp::start_with_random(random.clone()).await?;
    let (session, from) = app.register("ada@example.test").await?;
    let to = new_heap(&app, &session, &from, "Agent work").await?;
    for key in ["k3x9a", "k3x9a"] {
        random.push(bytes_of(key));
    }
    let item = app.new_item(&session, &from, json!({ "title": "Mine" })).await?;
    let url = app.item_url(id(&item)).await?;
    move_item(&app, &session, &url, &item["version"], &to).await?;

    // Act
    let newer = app.new_item(&session, &from, json!({ "title": "Newer" })).await?;
    let old = get(&app, &session, &url).await?;

    // Assert
    expect_that!(newer["key"], eq(&json!("k3x9a")));
    expect_that!(old.status, eq(StatusCode::OK));
    expect_that!(id(&old.body), eq(&id(&newer)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_moved_twice_is_found_by_every_former_path() -> Result<()> {
    // Arrange
    let w = world().await?;
    let third = new_heap(&w.app, &w.session, &w.from, "Third").await?;
    let first_url = w.item_url().await?;
    let moved = w.moved().await?;
    let second_url = w.app.item_url(id(&w.item)).await?;
    move_item(&w.app, &w.session, &second_url, &moved.body["version"], &third).await?;

    // Act
    let from_first = get(&w.app, &w.session, &first_url).await?;
    let from_second = get(&w.app, &w.session, &second_url).await?;

    // Assert
    for found in [&from_first, &from_second] {
        expect_that!(found.status, eq(StatusCode::OK));
        expect_that!(id(&found.body), eq(&id(&w.item)));
        expect_that!(found.body["heap_id"], eq(&json!(third)));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_old_path_after_a_move_to_another_team_leads_only_those_in_that_team() -> Result<()> {
    // Arrange
    let w = across().await?;
    let old_url = w.item_url().await?;
    w.moved().await?;

    // Act
    let by_ada = get(&w.app, &w.ada, &old_url).await?;
    let by_bob = get(&w.app, &w.bob, &old_url).await?;

    // Assert
    expect_that!(by_ada.status, eq(StatusCode::OK));
    expect_that!(by_ada.body["heap_id"], eq(&json!(w.to)));
    expect_that!(by_bob.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}
