//! Readable handles (design/readable-ids.md): usernames, team and heap slugs in paths, item keys, and personal
//! teams that stay their owner's alone.

mod common;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};
use service::ScriptedRandom;

const PASSWORD: &str = "a long test password";

async fn register(app: &TestApp, email: &str, username: &str) -> Result<common::TestResponse> {
    app.send(
        Method::POST,
        "/api/v1/auth/register",
        None,
        Some(json!({ "email": email, "username": username, "display_name": "Ada", "password": PASSWORD })),
    )
    .await
}

async fn post(app: &TestApp, session: &str, path: &str, body: Value) -> Result<common::TestResponse> {
    app.send(Method::POST, path, Some(session), Some(body)).await
}

#[gtest]
#[tokio::test]
async fn a_new_user_gets_a_personal_team_named_by_their_username_and_a_personal_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register_as("ada@example.test", "Ada").await?;

    // Act
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let list = app
        .send(
            Method::GET,
            "/api/v1/teams/ada/heaps/personal/items",
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(me.body["user"]["username"], eq(&json!("ada")));
    expect_that!(me.body["teams"][0]["slug"], eq(&json!("ada")));
    expect_that!(me.body["teams"][0]["personal"], eq(&json!(true)));
    expect_that!(me.body["teams"][0]["heaps"][0]["slug"], eq(&json!("personal")));
    expect_that!(me.body["teams"][0]["heaps"][0]["name"], eq(&json!("Personal")));
    expect_that!(list.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_username_must_be_well_formed_and_free_of_teams_too() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    post(&app, &ada, "/api/v1/teams", json!({ "name": "Platform" })).await?;

    // Act
    let shouting = register(&app, "grace@example.test", "Grace-Hopper").await?;
    let users_name = register(&app, "grace@example.test", "ada").await?;
    let teams_name = register(&app, "grace@example.test", "platform").await?;

    // Assert
    expect_that!(shouting.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(shouting.body["code"], eq(&json!("invalid_handle")));
    expect_that!(shouting.body["field"], eq(&json!("username")));
    expect_that!(shouting.body["hint"], eq(&json!("grace-hopper")));
    for taken in [&users_name, &teams_name] {
        expect_that!(taken.status, eq(StatusCode::CONFLICT));
        expect_that!(taken.body["code"], eq(&json!("handle_taken")));
        expect_that!(taken.body["field"], eq(&json!("username")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn teams_and_heaps_get_slugs_from_their_names_unless_given_one() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;

    // Act
    let first = post(&app, &ada, "/api/v1/teams", json!({ "name": "My Team" })).await?;
    let second = post(&app, &ada, "/api/v1/teams", json!({ "name": "My team!" })).await?;
    let chosen = post(
        &app,
        &ada,
        "/api/v1/teams",
        json!({ "name": "Ops", "slug": "infra-2027" }),
    )
    .await?;
    let taken = post(&app, &ada, "/api/v1/teams", json!({ "name": "Ops", "slug": "ada" })).await?;
    let heap = post(&app, &ada, "/api/v1/teams/my-team/heaps", json!({ "name": "Back log" })).await?;
    let again = post(&app, &ada, "/api/v1/teams/my-team/heaps", json!({ "name": "Back log" })).await?;
    let elsewhere = post(
        &app,
        &ada,
        "/api/v1/teams/my-team-2/heaps",
        json!({ "name": "Back log" }),
    )
    .await?;

    // Assert
    expect_that!(first.body["slug"], eq(&json!("my-team")));
    expect_that!(first.body["personal"], eq(&json!(false)));
    expect_that!(second.body["slug"], eq(&json!("my-team-2")));
    expect_that!(chosen.body["slug"], eq(&json!("infra-2027")));
    expect_that!(taken.status, eq(StatusCode::CONFLICT));
    expect_that!(taken.body["code"], eq(&json!("handle_taken")));
    expect_that!(heap.body["slug"], eq(&json!("back-log")));
    expect_that!(again.body["slug"], eq(&json!("back-log-2")));
    expect_that!(elsewhere.body["slug"], eq(&json!("back-log")));
    Ok(())
}

#[gtest]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn teams_and_heaps_made_at_once_from_one_name_get_different_slugs() -> Result<()> {
    // Arrange: each create looks for a free slug and then inserts it, so on MySQL they can all find `backlog` free.
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let heap = || post(&app, &ada, "/api/v1/teams/ada/heaps", json!({ "name": "Backlog" }));
    let team = || post(&app, &ada, "/api/v1/teams", json!({ "name": "Platform" }));

    // Act
    let (h1, h2, h3, t1, t2, t3) = tokio::join!(heap(), heap(), heap(), team(), team(), team());

    // Assert
    let mut heaps = Vec::new();
    for created in [h1?, h2?, h3?] {
        expect_that!(created.status, eq(StatusCode::CREATED), "{}", created.body);
        heaps.push(created.body["slug"].clone());
    }
    let mut teams = Vec::new();
    for created in [t1?, t2?, t3?] {
        expect_that!(created.status, eq(StatusCode::CREATED), "{}", created.body);
        teams.push(created.body["slug"].clone());
    }
    heaps.sort_by_key(ToString::to_string);
    teams.sort_by_key(ToString::to_string);
    expect_that!(
        heaps,
        eq(&vec![json!("backlog"), json!("backlog-2"), json!("backlog-3")])
    );
    expect_that!(
        teams,
        eq(&vec![json!("platform"), json!("platform-2"), json!("platform-3")])
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_malformed_team_or_heap_handle_is_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;

    // Act
    let mut answers = Vec::new();
    for path in [
        "/api/v1/teams/Ada",
        "/api/v1/teams/a--da",
        "/api/v1/teams/ada/heaps/Personal",
        "/api/v1/teams/ada/heaps/-personal/items",
    ] {
        answers.push((path, app.send(Method::GET, path, Some(&ada), None).await?));
    }

    // Assert
    for (path, answer) in &answers {
        expect_that!(answer.status, eq(StatusCode::NOT_FOUND), "{path}");
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_id_from_another_heap_of_the_team_is_not_found_under_this_one() -> Result<()> {
    // Arrange: Ada's personal team has two heaps, and everything is made in Work.
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let work = "/api/v1/teams/ada/heaps/work";
    let personal = "/api/v1/teams/ada/heaps/personal";
    post(&app, &ada, "/api/v1/teams/ada/heaps", json!({ "name": "Work" })).await?;
    let tag = post(&app, &ada, &format!("{work}/tags"), json!({ "name": "ops" })).await?;
    let view = post(&app, &ada, &format!("{work}/views"), json!({ "name": "Ready" })).await?;
    let parameter = post(
        &app,
        &ada,
        &format!("{work}/parameters"),
        json!({ "name": "Area", "options": ["Web"] }),
    )
    .await?;
    let id = |v: &Value| v["id"].as_str().map(str::to_owned).context("no id");
    let (tag_id, view_id, parameter_id) = (id(&tag.body)?, id(&view.body)?, id(&parameter.body)?);
    let option_id = id(&parameter.body["options"][0])?;

    // Act
    let renamed_tag = app
        .patch(
            &ada,
            &format!("{personal}/tags/{tag_id}"),
            &tag.body["version"],
            json!({ "name": "infra" }),
        )
        .await?;
    let archived_view = post(&app, &ada, &format!("{personal}/views/{view_id}/archive"), json!({})).await?;
    let archived_parameter = post(
        &app,
        &ada,
        &format!("{personal}/parameters/{parameter_id}/archive"),
        json!({}),
    )
    .await?;
    let archived_option = post(
        &app,
        &ada,
        &format!("{personal}/options/{option_id}/archive"),
        json!({}),
    )
    .await?;
    let tagged_item = post(
        &app,
        &ada,
        &format!("{personal}/items"),
        json!({ "title": "Task", "tag_ids": [tag_id] }),
    )
    .await?;

    // Assert
    for refused in [&renamed_tag, &archived_view, &archived_parameter, &archived_option] {
        expect_that!(refused.status, eq(StatusCode::NOT_FOUND), "{}", refused.body);
    }
    expect_that!(
        tagged_item.status,
        eq(StatusCode::UNPROCESSABLE_ENTITY),
        "{}",
        tagged_item.body
    );
    expect_that!(tagged_item.body["code"], eq(&json!("tag_not_in_heap")));
    let tags = app.send(Method::GET, &format!("{work}/tags"), Some(&ada), None).await?;
    expect_that!(tags.body["tags"][0]["name"], eq(&json!("ops")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_new_slug_moves_the_team_or_heap_to_a_new_path() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let team = post(&app, &ada, "/api/v1/teams", json!({ "name": "Platform" })).await?;
    let heap = post(&app, &ada, "/api/v1/teams/platform/heaps", json!({ "name": "Backlog" })).await?;
    post(&app, &ada, "/api/v1/teams/platform/heaps", json!({ "name": "Ideas" })).await?;

    // Act
    let renamed_team = app
        .patch(
            &ada,
            "/api/v1/teams/platform",
            &team.body["version"],
            json!({ "slug": "infra" }),
        )
        .await?;
    let renamed_heap = app
        .patch(
            &ada,
            "/api/v1/teams/infra/heaps/backlog",
            &heap.body["version"],
            json!({ "slug": "todo", "name": "To do" }),
        )
        .await?;
    let clash = app
        .patch(
            &ada,
            "/api/v1/teams/infra/heaps/todo",
            &renamed_heap.body["version"],
            json!({ "slug": "ideas" }),
        )
        .await?;
    let old = app
        .send(Method::GET, "/api/v1/teams/platform/heaps/backlog", Some(&ada), None)
        .await?;
    let new = app
        .send(Method::GET, "/api/v1/teams/infra/heaps/todo", Some(&ada), None)
        .await?;

    // Assert
    expect_that!(renamed_team.body["slug"], eq(&json!("infra")));
    expect_that!(renamed_heap.body["slug"], eq(&json!("todo")));
    expect_that!(renamed_heap.body["name"], eq(&json!("To do")));
    expect_that!(clash.status, eq(StatusCode::CONFLICT));
    expect_that!(clash.body["code"], eq(&json!("handle_taken")));
    expect_that!(clash.body["field"], eq(&json!("slug")));
    expect_that!(old.status, eq(StatusCode::NOT_FOUND));
    expect_that!(new.body["id"], eq(&heap.body["id"]));
    let id = heap.body["id"].as_str().context("no id")?;
    expect_that!(
        app.audit_entries(id).await?.last().map(|(_, diff)| diff.clone()),
        some(eq(&json!({
            "name": { "before": "Backlog", "after": "To do" },
            "slug": { "before": "backlog", "after": "todo" },
        })))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_personal_team_takes_no_members_and_keeps_its_slug() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let (grace, _) = app.register_as("grace@example.test", "Grace").await?;
    let grace_me = app.send(Method::GET, "/api/v1/me", Some(&grace), None).await?;
    let grace_id = grace_me.body["user"]["id"].clone();
    let team = app.send(Method::GET, "/api/v1/teams/ada", Some(&ada), None).await?;

    // Act
    let added = post(&app, &ada, "/api/v1/teams/ada/members", json!({ "user_id": grace_id })).await?;
    let left = post(&app, &ada, "/api/v1/teams/ada/leave", json!({})).await?;
    let removed = app
        .send(Method::DELETE, "/api/v1/teams/ada/members/ada", Some(&ada), None)
        .await?;
    let removed_outsider = app
        .send(Method::DELETE, "/api/v1/teams/ada/members/grace", Some(&ada), None)
        .await?;
    let reslugged = app
        .patch(
            &ada,
            "/api/v1/teams/ada",
            &team.body["version"],
            json!({ "slug": "countess" }),
        )
        .await?;
    let renamed = app
        .patch(
            &ada,
            "/api/v1/teams/ada",
            &team.body["version"],
            json!({ "name": "Ada's heaps" }),
        )
        .await?;

    // Assert
    expect_that!(team.body["personal"], eq(&json!(true)));
    expect_that!(team.body["allowed_actions"], eq(&json!(["rename", "create_heap"])));
    for refused in [&added, &left, &removed, &removed_outsider, &reslugged] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(refused.body["code"], eq(&json!("personal_team")));
    }
    expect_that!(renamed.status, eq(StatusCode::OK));
    expect_that!(renamed.body["slug"], eq(&json!("ada")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_are_named_by_username() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, _) = app.register_as("ada@example.test", "Ada").await?;
    let (grace, _) = app.register_as("grace@example.test", "Grace").await?;
    register(&app, "al@example.test", "al").await?;
    register(&app, "alan@example.test", "alan").await?;
    let grace_me = app.send(Method::GET, "/api/v1/me", Some(&grace), None).await?;
    post(&app, &ada, "/api/v1/teams", json!({ "name": "Platform" })).await?;
    let added = post(
        &app,
        &ada,
        "/api/v1/teams/platform/members",
        json!({ "user_id": grace_me.body["user"]["id"] }),
    )
    .await?;

    // Act
    let promoted = app
        .send(
            Method::PATCH,
            "/api/v1/teams/platform/members/grace",
            Some(&ada),
            Some(json!({ "role": "admin" })),
        )
        .await?;
    let nobody = app
        .send(
            Method::PATCH,
            "/api/v1/teams/platform/members/nobody",
            Some(&ada),
            Some(json!({ "role": "admin" })),
        )
        .await?;
    let found = app
        .send(Method::GET, "/api/v1/users?query=grace", Some(&ada), None)
        .await?;
    let short = app
        .send(Method::GET, "/api/v1/users?query=Al", Some(&ada), None)
        .await?;

    // Assert
    expect_that!(added.body["members"][1]["username"], eq(&json!("grace")));
    expect_that!(promoted.body["members"][1]["role"], eq(&json!("admin")));
    expect_that!(nobody.status, eq(StatusCode::NOT_FOUND));
    expect_that!(found.body["users"][0]["username"], eq(&json!("grace")));
    // A username too short to search by names finds only that user.
    expect_that!(short.status, eq(StatusCode::OK));
    expect_that!(
        short.body["users"]
            .as_array()
            .map(|users| users.iter().map(|u| u["username"].clone()).collect::<Vec<_>>()),
        some(eq(&vec![json!("al")]))
    );
    Ok(())
}

/// The random bytes that spell `key`, one per character of Crockford's base32.
fn bytes_of(key: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    key.bytes()
        .map(|c| {
            let i = ALPHABET.iter().position(|a| *a == c);
            #[allow(clippy::expect_used)]
            u8::try_from(i.expect("a key character")).expect("under 32")
        })
        .collect()
}

#[gtest]
#[tokio::test]
async fn a_new_item_skips_keys_without_a_digit_and_keys_its_heap_has_taken() -> Result<()> {
    // Arrange: the first draw goes to Ada's first item. Her second item draws a key with no digit, then the key she
    // already has, then a free one. Grace's heap doesn't have that first key yet, so she gets it too.
    let random = Arc::new(ScriptedRandom::default());
    let app = TestApp::start_with_random(random.clone()).await?;
    let (ada, ada_heap) = app.register_as("ada@example.test", "Ada").await?;
    let (grace, grace_heap) = app.register_as("grace@example.test", "Grace").await?;
    for key in ["k3x9a", "abcde", "k3x9a", "m4n5p", "k3x9a"] {
        random.push(bytes_of(key));
    }

    // Act
    let first = app.new_item(&ada, &ada_heap, json!({ "title": "First" })).await?;
    let second = app.new_item(&ada, &ada_heap, json!({ "title": "Second" })).await?;
    let elsewhere = app
        .new_item(&grace, &grace_heap, json!({ "title": "Elsewhere" }))
        .await?;

    // Assert
    expect_that!(first["key"], eq(&json!("k3x9a")));
    expect_that!(second["key"], eq(&json!("m4n5p")));
    expect_that!(elsewhere["key"], eq(&json!("k3x9a")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_are_found_by_their_key_within_their_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (ada, heap) = app.register_as("ada@example.test", "Ada").await?;
    let (grace, _) = app.register_as("grace@example.test", "Grace").await?;
    let item = app.new_item(&ada, &heap, json!({ "title": "Rotate keys" })).await?;
    let key = item["key"].as_str().context("item has no key")?;

    // Act
    let found = app
        .send(
            Method::GET,
            &format!("/api/v1/teams/ada/heaps/personal/items/{key}"),
            Some(&ada),
            None,
        )
        .await?;
    let elsewhere = app
        .send(
            Method::GET,
            &format!("/api/v1/teams/grace/heaps/personal/items/{key}"),
            Some(&grace),
            None,
        )
        .await?;
    let outsider = app
        .send(
            Method::GET,
            &format!("/api/v1/teams/ada/heaps/personal/items/{key}"),
            Some(&grace),
            None,
        )
        .await?;
    let malformed = app
        .send(
            Method::GET,
            "/api/v1/teams/ada/heaps/personal/items/NOPE!",
            Some(&ada),
            None,
        )
        .await?;

    // Assert
    expect_that!(key.len(), eq(5));
    expect_that!(key.chars().any(|c| c.is_ascii_digit()), eq(true));
    expect_that!(found.body["id"], eq(&item["id"]));
    for missing in [&elsewhere, &outsider, &malformed] {
        expect_that!(missing.status, eq(StatusCode::NOT_FOUND));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_local_user_is_me_with_a_personal_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start_local().await?;

    // Act
    let me = app.send(Method::GET, "/api/v1/me", None, None).await?;
    let list = app
        .send(Method::GET, "/api/v1/teams/me/heaps/personal/items", None, None)
        .await?;

    // Assert
    expect_that!(me.body["user"]["username"], eq(&json!("me")));
    expect_that!(me.body["teams"][0]["slug"], eq(&json!("me")));
    expect_that!(list.status, eq(StatusCode::OK));
    Ok(())
}
