//! `upheap item edit`: the fields it changes, the tags it adds and takes off, and what it says when nothing changed.

mod common;

use anyhow::Result;
use common::{HEAP, TestCli, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

#[gtest]
#[tokio::test]
async fn item_edit_changes_the_fields_given_and_clears_the_ones_asked() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    server
        .edit_item(
            &ada,
            HEAP,
            key,
            json!({ "due": { "date": "2027-03-12", "firm": true } }),
        )
        .await?;

    // Act
    let edited = cli
        .run(&[
            "item",
            "edit",
            key,
            "--title",
            "New title",
            "--effort",
            "m",
            "--no-due",
            "--no-impact",
        ])
        .await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq(&format!("Updated {key}: New title.\n")));
    let item = server.read_item(&ada, HEAP, key).await?;
    expect_that!(item["title"], eq("New title"));
    expect_that!(item["effort"], eq("m"));
    expect_that!(item["impact"], eq(&Value::Null));
    expect_that!(item["due"], eq(&Value::Null));
    expect_that!(item["description"], eq("Before they expire"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_edit_adds_and_takes_off_tags_by_name() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/tags",
            Some(json!({ "name": "Ops" })),
        )
        .await?;

    // Act
    let edited = cli
        .run(&["item", "edit", key, "--tag", "ops", "--untag", "INFRA"])
        .await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    let item = server.read_item(&ada, HEAP, key).await?;
    expect_that!(item["tags"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(item["tags"][0]["name"], eq("Ops"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_edit_with_nothing_to_change_makes_no_request() -> Result<()> {
    // Arrange: nobody is signed in and no server is set, so any request or setting lookup would fail.
    let server = TestServer::start().await?;
    let mut cli = TestCli::new(&server)?;
    cli.env.server = None;

    // Act
    let human = cli.run(&["item", "edit", "k3x9a"]).await;
    let quiet = cli.run(&["item", "edit", "k3x9a", "-q"]).await;
    let json = cli.run(&["item", "edit", "k3x9a", "--json"]).await;
    let malformed = cli.run(&["item", "edit", "a/b"]).await;

    // Assert
    expect_that!(
        human.stdout,
        eq("Nothing to change: give a field to change, such as --title or --due.\n")
    );
    expect_that!(human.exited(0), eq(true));
    expect_that!((quiet.stdout.as_str(), quiet.exited(0)), eq(("", true)));
    expect_that!((json.stdout.as_str(), json.exited(0)), eq(("", true)));
    expect_that!(malformed.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_edit_refuses_a_tag_it_is_also_told_to_take_off() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let edited = cli
        .run(&["item", "edit", key(&items[0])?, "--tag", "Infra", "--untag", "infra"])
        .await;

    expect_that!(edited.exited(2), eq(true));
    expect_that!(edited.stderr, eq("error: --tag and --untag both name Infra\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_edit_of_a_done_item_says_to_reopen_it_first() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    server.edit_item(&ada, HEAP, key, json!({ "state": "done" })).await?;

    // Act
    let edited = cli.run(&["item", "edit", key, "--title", "Too late"]).await;
    let as_json = cli.run(&["item", "edit", key, "--title", "Too late", "--json"]).await;

    // Assert
    expect_that!(edited.exited(3), eq(true));
    expect_that!(edited.stderr, contains_substring("(item_closed)"));
    expect_that!(
        edited.stderr,
        ends_with("hint: reopen it first with `upheap item reopen <item>`\n")
    );
    let problem: Value = serde_json::from_str(&as_json.stderr)?;
    expect_that!(problem["code"], eq("item_closed"));
    expect_that!(
        server.read_item(&ada, HEAP, key).await?["title"],
        eq("Rotate the staging certs")
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_edit_that_changes_nothing_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    // Act
    let same = cli.run(&["item", "edit", key, "--effort", "s"]).await;
    let nothing_to_clear = cli.run(&["item", "edit", key, "--no-due"]).await;
    let as_json = cli.run(&["item", "edit", key, "--effort", "m", "--json"]).await;

    // Assert
    expect_that!(same.stdout, eq(&format!("No change to {key}.\n")));
    expect_that!(nothing_to_clear.stdout, eq(&format!("No change to {key}.\n")));
    let edited: Value = serde_json::from_str(&as_json.stdout)?;
    expect_that!(edited["effort"], eq("m"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_edit_reads_the_description_from_stdin_and_clears_it_on_request() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.stdin = b"From stdin\r\n".to_vec();

    // Act
    let set = cli.run(&["item", "edit", key, "--description", "-", "-q"]).await;
    let after_set = server.read_item(&ada, HEAP, key).await?;
    let cleared = cli.run(&["item", "edit", key, "--no-description", "-q"]).await;
    let after_clear = server.read_item(&ada, HEAP, key).await?;

    // Assert
    expect_that!((set.stdout.as_str(), set.exited(0)), eq(("", true)));
    expect_that!(after_set["description"], eq("From stdin"));
    expect_that!(cleared.exited(0), eq(true));
    expect_that!(after_clear["description"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_flags_for_setting_and_clearing_a_field_exclude_each_other() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let edited = cli
        .run(&["item", "edit", key(&items[0])?, "--due", "2027-03-12", "--no-due"])
        .await;

    expect_that!(edited.exited(2), eq(true));
    Ok(())
}
