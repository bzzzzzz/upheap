//! `upheap item search`: items by title or key in every heap, open and closed.

mod common;

use anyhow::Result;
use common::{TestCli, TestServer, ada_with_items, key};
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
            "ada/personal/{}  Rotate the staging certs  open\n",
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
