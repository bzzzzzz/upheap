//! `upheap item place`: an item goes right above or below another in the heap's list, or back to where its fields put
//! it, and the command says so.

mod common;

use anyhow::Result;
use common::{TestCli, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::Value;

/// The keys of the heap's list, in order.
async fn listed(cli: &TestCli) -> Vec<String> {
    cli.run(&["item", "list", "--quiet"])
        .await
        .stdout
        .lines()
        .map(str::to_owned)
        .collect()
}

#[gtest]
#[tokio::test]
async fn placing_above_puts_the_item_right_above_the_other() -> Result<()> {
    // Arrange: the items rank by need, so the last one is the lowest.
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let (first, second, last) = (key(&items[0])?, key(&items[1])?, key(&items[2])?);

    // Act
    let placed = cli.run(&["item", "place", last, "--above", second]).await;
    let after = listed(&cli).await;

    // Assert
    expect_that!(placed.stdout, eq(&format!("{last} moved up, just above {second}.\n")));
    expect_that!(placed.stderr, eq(""));
    expect_that!(placed.exited(0), eq(true));
    expect_that!(after, elements_are![eq(first), eq(last), eq(second)]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn placing_below_puts_the_item_right_below_the_other() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let (first, second, last) = (key(&items[0])?, key(&items[1])?, key(&items[2])?);

    // Act
    let placed = cli.run(&["item", "place", first, "--below", second]).await;
    let after = listed(&cli).await;

    // Assert
    expect_that!(
        placed.stdout,
        eq(&format!("{first} moved down, just below {second}.\n"))
    );
    expect_that!(placed.exited(0), eq(true));
    expect_that!(after, elements_are![eq(second), eq(first), eq(last)]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reset_puts_the_item_back_and_json_and_quiet_print_the_answer() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let (first, second, last) = (key(&items[0])?, key(&items[1])?, key(&items[2])?);
    cli.run(&["item", "place", last, "--above", first]).await;

    // Act
    let reset = cli.run(&["item", "place", last, "--reset"]).await;
    let after = listed(&cli).await;
    let again = cli.run(&["item", "place", last, "--reset", "--json"]).await;
    let quiet = cli.run(&["item", "place", last, "--below", first, "--quiet"]).await;

    // Assert
    expect_that!(reset.stdout, eq(&format!("{last}'s place is reset.\n")));
    expect_that!(reset.exited(0), eq(true));
    expect_that!(after, elements_are![eq(first), eq(second), eq(last)]);
    let body: Value = serde_json::from_str(&again.stdout)?;
    expect_that!(body["placement"], eq(&Value::Null));
    expect_that!(quiet.stdout, eq(&format!("{last}\n")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn one_of_above_below_and_reset_is_needed() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let none = cli.run(&["item", "place", key(&items[0])?]).await;
    let both = cli
        .run(&["item", "place", key(&items[0])?, "--above", key(&items[1])?, "--reset"])
        .await;

    // Assert
    expect_that!(none.exited(0), eq(false));
    expect_that!(both.exited(0), eq(false));
    Ok(())
}
