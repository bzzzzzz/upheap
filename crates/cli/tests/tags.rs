//! `upheap tag list | add | edit`: what they print, what they change, and how they refuse.

mod common;

use anyhow::Result;
use common::{HEAP, TestCli, TestServer, TestUser, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

const TAGS: &str = "/teams/ada/heaps/personal/tags";

async fn add_tag(server: &TestServer, user: &TestUser, body: Value) -> Result<Value> {
    server.send(user, "POST", TAGS, Some(body)).await
}

/// The heap's tags as the server has them, by name.
async fn tags(server: &TestServer, user: &TestUser) -> Result<Vec<Value>> {
    let list = server.send(user, "GET", TAGS, None).await?;
    Ok(list["tags"].as_array().cloned().unwrap_or_default())
}

#[gtest]
#[tokio::test]
async fn tag_list_prints_each_tag_with_its_first_description_line_and_quiet_prints_names() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    add_tag(
        &server,
        &ada,
        json!({ "name": "Ops", "description": "Operations work\nand on-call" }),
    )
    .await?;

    // Act
    let human = cli.run(&["tag", "list"]).await;
    let quiet = cli.run(&["tag", "list", "-q"]).await;

    // Assert
    expect_that!(human.exited(0), eq(true));
    expect_that!(human.stdout, eq("Infra\nOps    Operations work\n"));
    expect_that!(quiet.stdout, eq("Infra\nOps\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_list_in_a_heap_without_tags_nudges_toward_tag_add() -> Result<()> {
    // Arrange: ada_with_items makes the Infra tag, so register on its own.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let listed = cli.run(&["tag", "list"]).await;
    let quiet = cli.run(&["tag", "list", "-q"]).await;

    // Assert
    expect_that!(listed.exited(0), eq(true));
    expect_that!(
        listed.stdout,
        eq("No tags in ada/personal yet; add one with `upheap tag add <name>`.\n")
    );
    expect_that!((quiet.stdout.as_str(), quiet.exited(0)), eq(("", true)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_list_json_is_the_servers_body() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let listed = cli.run(&["tag", "list", "--json"]).await;

    expect_that!(listed.exited(0), eq(true));
    let body: Value = serde_json::from_str(&listed.stdout)?;
    expect_that!(body["tags"].as_array().map(Vec::len), some(eq(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_add_creates_the_tag_with_its_description() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli
        .run(&["tag", "add", " Ops ", "--description", "Operations work"])
        .await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stdout, eq("Added tag \"Ops\".\n"));
    let all = tags(&server, &ada).await?;
    let ops = all.iter().find(|tag| tag["name"] == "Ops");
    expect_that!(
        ops.map(|tag| tag["description"].clone()),
        some(eq(&json!("Operations work")))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_add_with_a_taken_name_ignoring_case_is_refused_and_creates_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli.run(&["tag", "add", "INFRA"]).await;

    // Assert
    expect_that!(added.exited(3), eq(true));
    expect_that!(added.stderr, contains_substring("error:"));
    expect_that!(added.stdout, eq(""));
    expect_that!(tags(&server, &ada).await?.len(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_add_quiet_prints_the_name_and_json_the_created_tag() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let quiet = cli.run(&["tag", "add", "Ops", "-q"]).await;
    let json = cli.run(&["tag", "add", "Docs", "--json"]).await;

    // Assert
    expect_that!(quiet.stdout, eq("Ops\n"));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["name"], eq("Docs"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_add_by_someone_outside_the_heap_is_not_found() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (_, _) = ada_with_items(&server).await?;
    let bob = server.register("bob", "Bob").await?;
    let cli = cli_in_heap(&server, &bob).await?;

    // Act
    let added = cli.run(&["tag", "add", "Ops"]).await;

    // Assert
    expect_that!(added.exited(4), eq(true));
    expect_that!(added.stderr, contains_substring("hint: `upheap heap list`"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_edit_renames_a_tag_found_ignoring_case() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let edited = cli.run(&["tag", "edit", "infra", "--name", "Platform"]).await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq("Renamed tag \"Infra\" to \"Platform\".\n"));
    expect_that!(tags(&server, &ada).await?[0]["name"], eq("Platform"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_edit_by_id_clears_the_description() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let ops = add_tag(
        &server,
        &ada,
        json!({ "name": "Ops", "description": "Operations work" }),
    )
    .await?;
    let id = ops["id"].as_str().unwrap_or_default();

    // Act
    let edited = cli.run(&["tag", "edit", id, "--no-description"]).await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq("Updated tag \"Ops\".\n"));
    let all = tags(&server, &ada).await?;
    let ops = all.iter().find(|tag| tag["name"] == "Ops");
    expect_that!(ops.map(|tag| tag["description"].clone()), some(eq(&Value::Null)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_edit_to_the_same_name_says_nothing_changed() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let edited = cli.run(&["tag", "edit", "Infra", "--name", "Infra"]).await;

    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq("No change to tag \"Infra\".\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_edit_with_nothing_to_change_makes_no_request() -> Result<()> {
    // Arrange: nobody is signed in and no server is set, so any request or setting lookup would fail.
    let server = TestServer::start().await?;
    let mut cli = TestCli::new(&server)?;
    cli.env.server = None;

    // Act
    let human = cli.run(&["tag", "edit", "infra"]).await;
    let quiet = cli.run(&["tag", "edit", "infra", "-q"]).await;

    // Assert
    expect_that!(
        human.stdout,
        eq("Nothing to change: give --name, --description or --no-description.\n")
    );
    expect_that!(human.exited(0), eq(true));
    expect_that!((quiet.stdout.as_str(), quiet.exited(0)), eq(("", true)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_edit_of_an_unknown_tag_names_the_closest_without_offering_to_add_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let edited = cli.run(&["tag", "edit", "Nope", "--name", "X"]).await;

    // Assert
    expect_that!(edited.exited(3), eq(true));
    expect_that!(
        edited.stderr,
        eq(&format!("error: {HEAP} has no tag `Nope`; it has Infra\n"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn only_putting_an_unknown_tag_on_an_item_offers_to_add_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;

    // Act
    let tagged = cli.run(&["item", "edit", key, "--tag", "Big rock"]).await;
    let untagged = cli.run(&["item", "edit", key, "--untag", "infar"]).await;
    let padded = cli.run(&["item", "edit", key, "--untag", "infra "]).await;

    // Assert
    expect_that!(tagged.exited(3), eq(true));
    expect_that!(
        tagged.stderr,
        contains_substring(format!("hint: add it with `upheap tag add 'Big rock' --heap {HEAP}`"))
    );
    expect_that!(untagged.exited(3), eq(true));
    expect_that!(untagged.stderr, not(contains_substring("hint:")));
    expect_that!(padded.exited(0), eq(true));
    Ok(())
}
