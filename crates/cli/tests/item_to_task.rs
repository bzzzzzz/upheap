//! `upheap item to-task`: making a note a task, repeating it, `--json`, `--quiet` and a closed item.

mod common;

use anyhow::Result;
use common::{HEAP, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::Value;

#[gtest]
#[tokio::test]
async fn a_note_becomes_a_task_and_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    // Act
    let made = cli.run(&["item", "to-task", key]).await;

    // Assert
    expect_that!(
        made.stdout,
        eq(&format!("Made {key} a task: Rotate the staging certs.\n"))
    );
    expect_that!(made.stderr, eq(""));
    expect_that!(made.exited(0), eq(true));
    expect_that!(server.read_item(&ada, HEAP, key).await?["kind"], eq("task"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn repeating_it_writes_nothing_and_says_it_is_already_a_task() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.run(&["item", "to-task", key]).await;
    let version = server.read_item(&ada, HEAP, key).await?["version"].clone();

    // Act
    let again = cli.run(&["item", "to-task", key]).await;
    let as_json = cli.run(&["item", "to-task", key, "--json"]).await;

    // Assert
    expect_that!(again.stdout, eq(&format!("{key} is already a task.\n")));
    expect_that!(again.exited(0), eq(true));
    let read: Value = serde_json::from_str(&as_json.stdout)?;
    expect_that!(read["kind"], eq("task"));
    expect_that!(server.read_item(&ada, HEAP, key).await?["version"], eq(&version));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn json_prints_the_item_and_quiet_prints_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let json = cli.run(&["item", "to-task", key(&items[0])?, "--json"]).await;
    let quiet = cli.run(&["item", "to-task", key(&items[1])?, "--quiet"]).await;

    // Assert
    let made: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(made["kind"], eq("task"));
    expect_that!((quiet.stdout.as_str(), quiet.exited(0)), eq(("", true)));
    expect_that!(server.read_item(&ada, HEAP, key(&items[1])?).await?["kind"], eq("task"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_gets_the_servers_refusal() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.run(&["item", "done", key]).await;

    // Act
    let made = cli.run(&["item", "to-task", key]).await;

    // Assert
    expect_that!(made.exited(3), eq(true));
    expect_that!(made.stdout, eq(""));
    expect_that!(made.stderr, contains_substring("(item_closed)"));
    expect_that!(server.read_item(&ada, HEAP, key).await?["kind"], eq("note"));
    Ok(())
}
