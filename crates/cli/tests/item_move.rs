//! `upheap item move`: an item goes to another heap of its team, and the command says what it left behind.

mod common;

use std::sync::Arc;

use anyhow::Result;
use common::{HEAP, TestServer, TestUser, ada_with_items, cli_in_heap, key};
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

/// Makes the heap `slug` in Ada's personal team.
async fn add_heap(server: &TestServer, ada: &TestUser, name: &str) -> Result<()> {
    server
        .send(ada, "POST", "/teams/ada/heaps", Some(json!({ "name": name })))
        .await?;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_item_moves_and_the_command_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    add_heap(&server, &ada, "Other").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[2])?;

    // Act
    let moved = cli.run(&["item", "move", key, "ada/other"]).await;
    let there = server.read_item(&ada, "ada/other", key).await?;
    let gone = cli.run(&["item", "show", key]).await;

    // Assert
    expect_that!(moved.stdout, eq(&format!("Moved {key}: Tidy the wiki to ada/other.\n")));
    expect_that!(moved.stderr, eq(""));
    expect_that!(moved.exited(0), eq(true));
    expect_that!(there["id"], eq(&items[2]["id"]));
    expect_that!(there["version"], eq(&json!(2)));
    expect_that!(gone.exited(4), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_already_in_the_heap_is_left_alone() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    // Act
    let again = cli.run(&["item", "move", key, "ada/personal"]).await;
    let by_slug = cli.run(&["item", "move", key, "personal"]).await;

    // Assert
    expect_that!(again.stdout, eq(&format!("{key} is already in ada/personal.\n")));
    expect_that!(again.exited(0), eq(true));
    expect_that!(by_slug.stdout, eq(&format!("{key} is already in ada/personal.\n")));
    expect_that!(server.read_item(&ada, HEAP, key).await?["version"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_key_the_heap_already_has_is_replaced_and_quiet_prints_the_key_it_has_now() -> Result<()> {
    // Arrange: both heaps have an item keyed k3x9a and one keyed m4n5p, and the items that move then draw q7r8s and
    // t2v3w.
    let random = Arc::new(ScriptedRandom::default());
    let server = TestServer::start_with_random(random.clone()).await?;
    let ada = server.register("ada", "Ada").await?;
    add_heap(&server, &ada, "Other").await?;
    for key in ["k3x9a", "m4n5p", "k3x9a", "m4n5p", "q7r8s", "t2v3w"] {
        random.push(bytes_of(key));
    }
    let first = server.add_item(&ada, HEAP, json!({ "title": "Mine" })).await?;
    let second = server.add_item(&ada, HEAP, json!({ "title": "Also mine" })).await?;
    server.add_item(&ada, "ada/other", json!({ "title": "Theirs" })).await?;
    server
        .add_item(&ada, "ada/other", json!({ "title": "Theirs too" }))
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let moved = cli.run(&["item", "move", key(&first)?, "ada/other"]).await;
    let quiet = cli.run(&["item", "move", key(&second)?, "ada/other", "-q"]).await;

    // Assert
    expect_that!(key(&first)?, eq("k3x9a"));
    expect_that!(key(&second)?, eq("m4n5p"));
    expect_that!(
        moved.stdout,
        eq("Moved k3x9a: Mine to ada/other.\nIts key is now q7r8s: the heap already had k3x9a.\n")
    );
    expect_that!(quiet.stdout, eq("t2v3w\n"));
    expect_that!(quiet.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tags_and_values_the_heap_has_no_match_for_are_named() -> Result<()> {
    // Arrange: the first item has the tag Infra, and Area: Backend.
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    add_heap(&server, &ada, "Other").await?;
    let area = server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/parameters",
            Some(json!({ "name": "Area", "options": ["Backend", "Web"] })),
        )
        .await?;
    let item = server
        .edit_item(
            &ada,
            HEAP,
            key(&items[0])?,
            json!({ "parameter_values": { area["id"].as_str().unwrap_or_default(): area["options"][0]["id"] } }),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let moved = cli.run(&["item", "move", key(&item)?, "ada/other"]).await;

    // Assert
    expect_that!(
        moved.stdout,
        eq(&format!(
            "Moved {}: Rotate the staging certs to ada/other.\n\
             Dropped tags the heap doesn't have: Infra.\n\
             Dropped values the heap has no match for: Area: Backend.\n",
            key(&item)?
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_of_another_team_is_refused_with_the_servers_message() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let team = server
        .send(&ada, "POST", "/teams", Some(json!({ "name": "Shared" })))
        .await?;
    let slug = team["slug"].as_str().unwrap_or_default().to_owned();
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/{slug}/heaps"),
            Some(json!({ "name": "Work" })),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    // Act
    let refused = cli.run(&["item", "move", key, &format!("{slug}/work")]).await;

    // Assert
    expect_that!(refused.exited(3), eq(true));
    expect_that!(refused.stdout, eq(""));
    expect_that!(
        refused.stderr,
        contains_substring("the item can only move to another heap of its team")
    );
    expect_that!(refused.stderr, contains_substring("(heap_not_in_team)"));
    expect_that!(server.read_item(&ada, HEAP, key).await?["version"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn json_prints_the_item_as_the_server_sent_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    add_heap(&server, &ada, "Other").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[1])?;

    // Act
    let moved = cli.run(&["item", "move", key, "ada/other", "--json"]).await;

    // Assert
    let read: Value = serde_json::from_str(&moved.stdout)?;
    expect_that!(read["key"], eq(key));
    expect_that!(read, eq(&server.read_item(&ada, "ada/other", key).await?));
    expect_that!(moved.exited(0), eq(true));
    Ok(())
}
