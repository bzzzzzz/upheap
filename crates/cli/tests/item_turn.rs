//! `upheap item send | take-back`: sending an item to an agent, and taking it back, twice too.

mod common;

use anyhow::Result;
use common::{HEAP, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::Value;

#[gtest]
#[tokio::test]
async fn send_opens_a_turn_to_the_next_stable_state_and_says_where_it_goes() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let (first, second) = (key(&items[0])?, key(&items[1])?);

    // Act
    let sent = cli.run(&["item", "send", first]).await;
    let as_json = cli.run(&["item", "send", second, "--json"]).await;

    // Assert
    expect_that!(
        sent.stdout,
        eq(&format!(
            "Sent {first}: Rotate the staging certs. Turn: to with context.\n"
        ))
    );
    expect_that!(sent.exited(0), eq(true));
    let turn = server.read_item(&ada, HEAP, first).await?["turn"].clone();
    expect_that!(turn["reach"], eq("one"));
    expect_that!(turn["goal"], eq("with_context"));
    let printed: Value = serde_json::from_str(&as_json.stdout)?;
    expect_that!(printed["key"], eq(second));
    expect_that!(printed["turn"]["goal"], eq("with_context"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn send_all_the_way_with_a_line_from_stdin_tells_the_agent() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.stdin = b"Use the new CA.\n".to_vec();

    // Act
    let sent = cli.run(&["item", "send", key, "--all-the-way", "--line", "-"]).await;

    // Assert
    expect_that!(
        sent.stdout,
        eq(&format!(
            "Sent {key}: Rotate the staging certs. Turn: to defined, all the way. \"Use the new CA.\".\n"
        ))
    );
    let turn = server.read_item(&ada, HEAP, key).await?["turn"].clone();
    expect_that!(turn["reach"], eq("all_the_way"));
    expect_that!(turn["line"], eq("Use the new CA."));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn take_back_ends_the_turn_and_repeating_it_is_no_error() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.run(&["item", "send", key, "-q"]).await;

    // Act
    let taken = cli.run(&["item", "take-back", key]).await;
    let again = cli.run(&["item", "take-back", key]).await;

    // Assert
    let line = format!("No agent has {key}: Rotate the staging certs.\n");
    expect_that!(taken.stdout, eq(&line));
    expect_that!(taken.exited(0), eq(true));
    expect_that!(again.stdout, eq(&line));
    expect_that!(again.exited(0), eq(true));
    expect_that!(server.read_item(&ada, HEAP, key).await?["turn"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_item_is_not_found() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let sent = cli.run(&["item", "send", "zzzzz"]).await;

    // Assert
    expect_that!(sent.exited(4), eq(true));
    expect_that!(
        sent.stderr,
        contains_substring("`upheap item list` lists the heap's items")
    );
    Ok(())
}
