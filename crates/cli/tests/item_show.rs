//! `upheap item show`: the item in full, naming it by key, path or link, and its history.

mod common;

use anyhow::{Context, Result};
use common::{HEAP, TestCli, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

/// The value on the line that starts with `label`.
fn value_of<'a>(stdout: &'a str, label: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .map(str::trim_start)
}

#[gtest]
#[tokio::test]
async fn item_show_prints_the_set_fields_and_leaves_out_the_unset() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .edit_item(
            &ada,
            HEAP,
            key(&items[0])?,
            json!({ "due": { "date": "2027-03-12", "firm": true }, "description": "Before they expire\nin March" }),
        )
        .await?;

    // Act
    let shown = cli.run(&["item", "show", key(&items[0])?]).await;
    let bare = cli.run(&["item", "show", key(&items[2])?]).await;

    // Assert
    expect_that!(shown.exited(0), eq(true));
    expect_that!(shown.stderr, eq(""));
    expect_that!(
        shown.stdout.lines().next(),
        some(eq(&format!("{}  Rotate the staging certs", key(&items[0])?)))
    );
    expect_that!(value_of(&shown.stdout, "Kind"), some(eq("note")));
    expect_that!(value_of(&shown.stdout, "Stage"), some(eq("as written")));
    expect_that!(value_of(&shown.stdout, "State"), some(eq("open")));
    expect_that!(value_of(&shown.stdout, "Need"), some(eq("critical")));
    expect_that!(value_of(&shown.stdout, "Effort"), some(eq("s")));
    expect_that!(value_of(&shown.stdout, "Impact"), some(eq("l")));
    expect_that!(value_of(&shown.stdout, "Tags"), some(eq("Infra")));
    expect_that!(value_of(&shown.stdout, "Due"), some(ends_with("(firm)")));
    expect_that!(value_of(&shown.stdout, "Created"), some(ends_with("by Ada")));
    expect_that!(
        shown.stdout,
        ends_with("\n\nContext\n  Before they expire\n  in March\n")
    );
    expect_that!(bare.stdout, not(contains_substring("Context")));
    expect_that!(value_of(&bare.stdout, "Effort"), none());
    expect_that!(value_of(&bare.stdout, "Tags"), none());
    expect_that!(value_of(&bare.stdout, "Need"), some(eq("idea")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_says_an_agent_has_the_item_and_where_it_is_going() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    let quiet_key = self::key(&items[1])?;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{key}/turn"),
            Some(json!({ "reach": "all_the_way", "line": "Use the new CA." })),
        )
        .await?;

    // Act
    let shown = cli.run(&["item", "show", key, "--history"]).await;
    let without = cli.run(&["item", "show", quiet_key]).await;

    // Assert
    expect_that!(shown.exited(0), eq(true));
    expect_that!(value_of(&shown.stdout, "State"), some(eq("open · with an agent")));
    expect_that!(
        shown.stdout,
        contains_substring("\nTurn: to with context, all the way. \"Use the new CA.\".\n")
    );
    let (_, history) = shown.stdout.split_once("\nHistory\n").context("a History section")?;
    expect_that!(history.lines().next(), some(contains_substring("Ada opened a turn on")));
    expect_that!(value_of(&without.stdout, "State"), some(eq("open")));
    expect_that!(without.stdout, not(contains_substring("Turn:")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_prints_crlf_and_tabs_in_a_description_as_text() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    server
        .edit_item(&ada, HEAP, key, json!({ "description": "one\r\n\ttwo\r\nthree" }))
        .await?;

    let shown = cli.run(&["item", "show", key]).await;

    expect_that!(shown.exited(0), eq(true));
    expect_that!(shown.stdout, ends_with("\n\nContext\n  one\n      two\n  three\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_finds_an_item_by_path_or_pasted_link_without_a_heap() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let key = key(&items[1])?;

    // Act
    let by_path = cli.run(&["item", "show", &format!("ada/personal/{key}"), "-q"]).await;
    let by_link = cli
        .run(&[
            "item",
            "show",
            &format!("http://whatever.test/h/ada/personal/{key}/?utm=x"),
            "-q",
        ])
        .await;
    let by_key = cli.run(&["item", "show", key, "-q"]).await;

    // Assert
    expect_that!(by_path.stdout, eq(&format!("{key}\n")));
    expect_that!(by_link.stdout, eq(&format!("{key}\n")));
    expect_that!(by_key.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_of_an_unknown_key_points_to_the_list() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let shown = cli.run(&["item", "show", "zzzzz"]).await;

    expect_that!(shown.exited(4), eq(true));
    expect_that!(shown.stdout, eq(""));
    expect_that!(shown.stderr, starts_with("error: no item `zzzzz` in ada/personal"));
    expect_that!(
        shown.stderr,
        contains_substring("hint: `upheap item list` lists the heap's items")
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_with_malformed_names_and_a_clashing_heap_is_a_usage_error() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    let malformed = cli.run(&["item", "show", "a/b"]).await;
    let clash = cli
        .run(&["item", "show", &format!("ada/personal/{key}"), "--heap", "other/heap"])
        .await;
    let same = cli
        .run(&[
            "item",
            "show",
            &format!("ada/personal/{key}"),
            "--heap",
            "ada/personal",
            "-q",
        ])
        .await;

    expect_that!(malformed.exited(2), eq(true));
    expect_that!(
        malformed.stderr,
        eq("error: `a/b` isn't an item: give its key, team/heap/key, or a link to it\n")
    );
    expect_that!(clash.exited(2), eq(true));
    expect_that!(
        clash.stderr,
        eq(&format!(
            "error: `ada/personal/{key}` is in ada/personal, but --heap says other/heap; leave out one of them\n"
        ))
    );
    expect_that!(same.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_history_lists_the_changes_newest_first() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    server
        .edit_item(&ada, HEAP, key, json!({ "title": "Rotate the prod certs" }))
        .await?;

    // Act
    let shown = cli.run(&["item", "show", key, "--history"]).await;

    // Assert
    expect_that!(shown.exited(0), eq(true));
    let (_, history) = shown.stdout.split_once("\nHistory\n").context("a History section")?;
    let lines: Vec<&str> = history.lines().collect();
    expect_that!(lines[0], contains_substring("Ada edited item"));
    expect_that!(
        lines[1],
        eq("  Title: Rotate the staging certs → Rotate the prod certs")
    );
    expect_that!(lines.iter().any(|line| line.contains("Ada added item")), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_json_with_history_is_one_object_of_both_bodies() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    // Act
    let item_only = cli.run(&["item", "show", key, "--json"]).await;
    let both = cli.run(&["item", "show", key, "--json", "--history"]).await;
    let quiet = cli.run(&["item", "show", key, "--quiet", "--history"]).await;

    // Assert
    let item: Value = serde_json::from_str(&item_only.stdout)?;
    expect_that!(item["key"], eq(key));
    let combined: Value = serde_json::from_str(&both.stdout)?;
    expect_that!(combined["item"]["key"], eq(key));
    expect_that!(combined["history"]["entries"].as_array().map(Vec::len), some(gt(0)));
    expect_that!(quiet.stdout, eq(&format!("{key}\n")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_gives_the_reason_a_blocked_item_has() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .edit_item(
            &ada,
            HEAP,
            key(&items[1])?,
            json!({ "state": "blocked", "blocked_reason": "Waits for the vendor's quote" }),
        )
        .await?;

    // Act
    let shown = cli.run(&["item", "show", key(&items[1])?]).await;
    let other = cli.run(&["item", "show", key(&items[0])?]).await;

    // Assert
    expect_that!(
        value_of(&shown.stdout, "Blocked because"),
        some(eq("Waits for the vendor's quote"))
    );
    expect_that!(value_of(&other.stdout, "Blocked because"), none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_names_the_blocker() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .edit_item(
            &ada,
            HEAP,
            key(&items[1])?,
            json!({ "state": "blocked", "blocked_by_item_id": items[0]["id"] }),
        )
        .await?;

    // Act
    let shown = cli.run(&["item", "show", key(&items[1])?]).await;

    // Assert
    expect_that!(value_of(&shown.stdout, "State"), some(eq("blocked")));
    expect_that!(
        value_of(&shown.stdout, "Blocked by"),
        some(eq(&format!("{}  Rotate the staging certs (open)", key(&items[0])?)))
    );
    Ok(())
}
