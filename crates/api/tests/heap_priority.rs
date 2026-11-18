//! A person's priority for a heap (`PUT …/priority`): reading and setting it, who may, its audit entry, and
//! the priorities a heap made from a note starts with.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// An admin's session, a member's session, and the shared team and heap.
struct Setup {
    app: TestApp,
    admin: String,
    member: String,
    team: String,
    heap: String,
}

async fn setup() -> Result<Setup> {
    let app = TestApp::start().await?;
    let (admin, _) = app.register("ada@example.test").await?;
    let (member, _) = app.register("bob@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let member_id = me.body["user"]["id"].as_str().context("no user")?.to_owned();
    app.add_member(&team, &member_id, Role::Member).await?;
    Ok(Setup {
        app,
        admin,
        member,
        team,
        heap,
    })
}

impl Setup {
    async fn read(&self, session: &str) -> Result<Value> {
        let response = self
            .app
            .send(Method::GET, &self.app.heap_url(&self.heap).await?, Some(session), None)
            .await?;
        Ok(response.body)
    }

    /// The priority the user of `session` reads for the heap with `heap_id`.
    async fn priority_of(&self, session: &str, heap_id: &str) -> Result<Value> {
        let url = self.app.heap_url(heap_id).await?;
        let response = self.app.send(Method::GET, &url, Some(session), None).await?;
        Ok(response.body["priority"].clone())
    }

    async fn user_id(&self, session: &str) -> Result<String> {
        let me = self.app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
        Ok(me.body["user"]["id"].as_str().context("no user")?.to_owned())
    }
}

#[gtest]
#[tokio::test]
async fn a_new_heap_reads_priority_1_and_each_person_sets_their_own() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let fresh = s.read(&s.admin).await?;

    // Act
    let set = s.app.set_priority(&s.admin, &s.heap, 3).await?;
    let admin_reads = s.read(&s.admin).await?;
    let member_reads = s.read(&s.member).await?;
    let member_sets = s.app.set_priority(&s.member, &s.heap, 5).await?;
    let admin_still = s.read(&s.admin).await?;

    // Assert
    expect_that!(fresh["priority"], eq(&json!(1)));
    expect_that!(set.status, eq(StatusCode::OK));
    expect_that!(set.body["priority"], eq(&json!(3)));
    expect_that!(admin_reads["priority"], eq(&json!(3)));
    expect_that!(member_reads["priority"], eq(&json!(1)));
    expect_that!(member_sets.body["priority"], eq(&json!(5)));
    expect_that!(admin_still["priority"], eq(&json!(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_priority_out_of_range_is_refused_and_saves_nothing() -> Result<()> {
    // Arrange
    let s = setup().await?;
    s.app.set_priority(&s.admin, &s.heap, 4).await?;

    // Act
    let mut refused = Vec::new();
    for raw in [0, 11, -1, i64::MAX] {
        refused.push(s.app.set_priority(&s.admin, &s.heap, raw).await?);
    }
    let path = format!("{}/priority", s.app.heap_url(&s.heap).await?);
    let unknown = s
        .app
        .send(
            Method::PUT,
            &path,
            Some(&s.admin),
            Some(json!({ "priority": 2, "weight": 3 })),
        )
        .await?;

    // Assert
    for response in &refused {
        expect_that!(response.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(response.body["code"], eq(&json!("priority_out_of_range")));
    }
    expect_that!(unknown.status.is_client_error(), eq(true));
    expect_that!(s.read(&s.admin).await?["priority"], eq(&json!(4)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_priority_can_be_set_on_an_archived_heap() -> Result<()> {
    // Arrange
    let s = setup().await?;
    s.app.set_heap_archived(&s.heap, true).await?;

    // Act
    let set = s.app.set_priority(&s.member, &s.heap, 6).await?;

    // Assert
    expect_that!(set.status, eq(StatusCode::OK));
    expect_that!(set.body["priority"], eq(&json!(6)));
    expect_that!(
        set.body["allowed_actions"].as_array().context("actions")?,
        contains(eq(&json!("set_priority")))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_of_another_team_is_not_found() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let (outsider, _) = s.app.register("eve@example.test").await?;

    // Act
    let set = s.app.set_priority(&outsider, &s.heap, 3).await?;

    // Assert
    expect_that!(set.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn setting_the_same_value_twice_records_one_entry_in_the_personal_team() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let user = s.user_id(&s.member).await?;

    // Act
    let first = s.app.set_priority(&s.member, &s.heap, 2).await?;
    let again = s.app.set_priority(&s.member, &s.heap, 2).await?;
    let default_again = s.app.set_priority(&s.admin, &s.heap, 1).await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(default_again.status, eq(StatusCode::OK));
    let entries = s.app.audit_entries(&user).await?;
    let changed: Vec<_> = entries
        .iter()
        .filter(|(action, _)| action == "heap_priority_changed")
        .collect();
    expect_that!(changed.len(), eq(1));
    expect_that!(changed[0].1["priority"], eq(&json!({ "before": 1, "after": 2 })));
    expect_that!(changed[0].1["heap_id"], eq(&json!(s.heap)));
    expect_that!(changed[0].1["name"], eq(&json!("Shared")));
    // Nothing in the heap's own team log, and the one-row table holds one row for the member.
    expect_that!(s.app.count_rows("heap_priorities").await?, eq(1));
    let team_log = s.app.audit_entries(&s.team).await?;
    expect_that!(
        team_log.iter().any(|(action, _)| action == "heap_priority_changed"),
        eq(false)
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_priority_change_in_the_personal_log_names_the_heap_of_another_team() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let me = s.app.send(Method::GET, "/api/v1/me", Some(&s.member), None).await?;
    let username = me.body["user"]["username"].as_str().context("no username")?.to_owned();

    // Act
    s.app.set_priority(&s.member, &s.heap, 4).await?;
    let log = s
        .app
        .send(
            Method::GET,
            &format!("/api/v1/teams/{username}/audit"),
            Some(&s.member),
            None,
        )
        .await?;

    // Assert
    expect_that!(log.status, eq(StatusCode::OK));
    expect_that!(log.body["entries"][0]["action"], eq(&json!("heap_priority_changed")));
    expect_that!(
        log.body["entries"][0]["changes"],
        eq(&json!([
            { "field": "heap_id", "label": "Heap", "before": null, "after": "Shared", "text_diff": null },
            { "field": "priority", "label": "Priority", "before": "1", "after": "4", "text_diff": null },
        ]))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_made_from_a_note_starts_with_each_persons_priority_for_the_notes_heap() -> Result<()> {
    // Arrange: the admin and the member set a priority for the shared heap; a third member never did.
    let s = setup().await?;
    let (other, _) = s.app.register("cy@example.test").await?;
    let other_id = s.user_id(&other).await?;
    s.app.add_member(&s.team, &other_id, Role::Member).await?;
    s.app.set_priority(&s.admin, &s.heap, 4).await?;
    s.app.set_priority(&s.member, &s.heap, 6).await?;
    let note = s
        .app
        .new_item(
            &s.admin,
            &s.heap,
            json!({ "title": "Move the blog", "need": "expected" }),
        )
        .await?;
    let heaps = format!("{}/heaps", s.app.team_url(&s.team).await?);

    // Act
    let made = s
        .app
        .send(
            Method::POST,
            &heaps,
            Some(&s.admin),
            Some(json!({ "name": "Blog move", "origin_id": note["id"] })),
        )
        .await?;
    let plain = s
        .app
        .send(Method::POST, &heaps, Some(&s.admin), Some(json!({ "name": "Plain" })))
        .await?;

    // Assert: the made heap reads each person's priority for the note's heap, and the plain one the default.
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(made.body["priority"], eq(&json!(4)));
    let made_id = made.body["id"].as_str().context("no heap id")?;
    expect_that!(s.priority_of(&s.admin, made_id).await?, eq(&json!(4)));
    expect_that!(s.priority_of(&s.member, made_id).await?, eq(&json!(6)));
    expect_that!(s.priority_of(&other, made_id).await?, eq(&json!(1)));
    expect_that!(plain.status, eq(StatusCode::CREATED));
    expect_that!(plain.body["priority"], eq(&json!(1)));
    // The two rows set on the shared heap and their two copies; nobody's personal log has a change.
    expect_that!(s.app.count_rows("heap_priorities").await?, eq(4));
    let member_id = s.user_id(&s.member).await?;
    let changes = s.app.audit_entries(&member_id).await?;
    expect_that!(
        changes
            .iter()
            .filter(|(action, _)| action == "heap_priority_changed")
            .count(),
        eq(1)
    );
    Ok(())
}
