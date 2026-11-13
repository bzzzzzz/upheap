//! `upheap guide show`.

mod common;

use anyhow::Result;
use common::{TestCli, TestServer};
use googletest::prelude::*;

#[gtest]
#[tokio::test]
async fn guide_show_prints_the_commands_and_the_instructions_with_nobody_signed_in() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;

    // Act
    let shown = cli.run(&["guide", "show"]).await;

    // Assert
    expect_that!(shown.exited(0), eq(true));
    expect_that!(shown.stderr, eq(""));
    expect_that!(shown.stdout, starts_with("upheap "));
    expect_that!(
        shown.stdout,
        contains_substring("\nitem: Items in a heap\n  upheap item add <TITLE> [heap]: Add an item\n")
    );
    expect_that!(shown.stdout, contains_substring("\nFor agents\n"));
    expect_that!(shown.stdout, contains_substring("--all-heaps"));
    expect_that!(
        shown.stdout,
        contains_substring("  upheap question review <ITEM> <LINK> <TEXT>")
    );
    expect_that!(
        shown.stdout,
        contains_substring("  upheap question propose-done <ITEM> <TEXT>")
    );
    expect_that!(shown.stdout, contains_substring("An agent never closes a task"));
    expect_that!(shown.stdout, ends_with("  upheap item done k3x9a\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn guide_show_works_where_the_config_file_is_broken() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;
    std::fs::write(cli.config_dir.path().join("config.toml"), "not a config file")?;

    // Act
    let shown = cli.run(&["guide", "show"]).await;
    let other = cli.run(&["heap", "list"]).await;

    // Assert
    expect_that!(shown.exited(0), eq(true));
    expect_that!(shown.stdout, contains_substring("\nFor agents\n"));
    expect_that!(other.exited(6), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn guide_show_lists_the_commands_of_a_group_with_their_arguments() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;

    // Act
    let shown = cli.run(&["guide", "show"]).await;

    // Assert
    expect_that!(
        shown.stdout,
        contains_substring(
            "  upheap parameter option add <PARAM> <LABEL> [heap]: Add an option as the lowest priority\n"
        )
    );
    expect_that!(shown.stdout, contains_substring("--replace-with <OPTION>"));
    expect_that!(
        shown.stdout,
        contains_substring("  upheap work run [heap]: Work the queue: claim an item, start an agent tool on it,")
    );
    expect_that!(
        shown.stdout,
        contains_substring("      --tool <TOOL>           The agent tool to start on each item\n")
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn guide_show_takes_no_server_flags() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;

    // Act
    let json = cli.run(&["guide", "show", "--json"]).await;

    // Assert
    expect_that!(json.exited(2), eq(true));
    expect_that!(json.stdout, eq(""));
    Ok(())
}
