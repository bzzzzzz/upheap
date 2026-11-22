//! `upheap item search`: items by title or key in every heap, open and closed.

mod common;

use anyhow::Result;
use common::{TestCli, TestServer, ada_with_items, cli_in_heap, key, lines_with};
use googletest::prelude::*;
use serde_json::Value;

#[gtest]
#[tokio::test]
async fn item_search_lists_the_name_title_and_state_of_each_match() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let found = cli.run(&["item", "search", "STAGING"]).await;

    // Assert
    expect_that!(
        found.stdout,
        eq(&format!(
            "ada/personal/{}  Rotate the staging certs  Open\n",
            key(&items[0])?
        ))
    );
    expect_that!(found.stderr, eq(""));
    expect_that!(found.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_search_finds_an_item_by_its_key() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let found = cli.run(&["item", "search", "-q", key(&items[1])?]).await;

    // Assert
    expect_that!(found.stdout, eq(&format!("ada/personal/{}\n", key(&items[1])?)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_search_with_no_match_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let found = cli.run(&["item", "search", "nothing like it"]).await;

    // Assert
    expect_that!(found.stdout, eq("No items in your heaps match \"nothing like it\".\n"));
    expect_that!(found.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_search_json_is_the_servers_body() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let expected = server.send(&ada, "GET", "/me/search?q=wiki", None).await?;

    // Act
    let found = cli.run(&["item", "search", "--json", "wiki"]).await;

    // Assert
    let printed: Value = serde_json::from_str(&found.stdout)?;
    expect_that!(printed, eq(&expected));
    expect_that!(printed["rows"].as_array().map(Vec::len), some(eq(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_search_needs_the_text() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;

    // Act
    let found = cli.run(&["item", "search"]).await;

    // Assert
    expect_that!(found.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_search_dims_closed_rows_on_a_terminal_and_prints_plain_text_otherwise() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["item", "done", key(&items[1])?]).await;
    let faint = "\u{1b}[2m";

    // Act
    let piped = cli.run(&["item", "search", "the"]).await;
    cli.env.stdout_is_terminal = true;
    let on_terminal = cli.run(&["item", "search", "the"]).await;
    cli.env.no_color = true;
    let no_color = cli.run(&["item", "search", "the"]).await;

    // Assert
    let open = lines_with(&on_terminal.stdout, "staging");
    let closed = lines_with(&on_terminal.stdout, "migration");
    expect_that!(open, elements_are![not(contains_substring(faint))]);
    expect_that!(closed, elements_are![starts_with(faint)]);
    expect_that!(closed, elements_are![contains_substring("Done")]);
    expect_that!(piped.stdout, not(contains_substring("\u{1b}")));
    expect_that!(no_color.stdout, eq(&piped.stdout));
    expect_that!(
        lines_with(&piped.stdout, "migration"),
        elements_are![eq(&format!(
            "ada/personal/{}  Write the migration guide  Done",
            key(&items[1])?
        ))]
    );
    Ok(())
}
