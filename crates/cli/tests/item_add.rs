//! `upheap item add`: the flags, where the item landed, and what a refusal leaves behind (nothing).

mod common;

use anyhow::{Context, Result};
use common::{HEAP, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::Value;

/// The key in `Added "<title>" (<key>), now …`.
fn added_key(stdout: &str) -> Option<&str> {
    stdout.split_once(" (")?.1.split_once(')').map(|(key, _)| key)
}

async fn item_count(server: &TestServer, ada: &common::TestUser) -> Result<usize> {
    let list = server.send(ada, "GET", "/teams/ada/heaps/personal/items", None).await?;
    Ok(list["ranked"].as_array().context("ranked")?.len() + list["pinned"].as_array().context("pinned")?.len())
}

#[gtest]
#[tokio::test]
async fn item_add_creates_the_item_and_says_where_it_landed() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli
        .run(&[
            "item",
            "add",
            "Rotate certs",
            "--effort",
            "s",
            "--impact",
            "l",
            "--need",
            "critical",
            "--due",
            "2027-03-12",
            "--tag",
            "infra",
        ])
        .await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stderr, eq(""));
    expect_that!(added.stdout, starts_with("Added \"Rotate certs\" ("));
    expect_that!(added.stdout, ends_with(" of 4.\n"));
    expect_that!(added.stdout, contains_substring("), now "));
    let new_key = added_key(&added.stdout).context("a key in the output")?;
    expect_that!(items.iter().any(|item| key(item).ok() == Some(new_key)), eq(false));
    let item = server.read_item(&ada, HEAP, new_key).await?;
    expect_that!(item["title"], eq("Rotate certs"));
    expect_that!(item["effort"], eq("s"));
    expect_that!(item["impact"], eq("l"));
    expect_that!(item["need"], eq("critical"));
    expect_that!(item["due"]["date"], eq("2027-03-12"));
    expect_that!(item["due"]["firm"], eq(true));
    expect_that!(item["tags"][0]["name"], eq("Infra"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_without_a_need_asks_for_nice_to_have() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let added = cli.run(&["item", "add", "Plain", "--quiet"]).await;

    let item = server.read_item(&ada, HEAP, added.stdout.trim()).await?;
    expect_that!(item["need"], eq("nice_to_have"));
    expect_that!(item["effort"], eq(&Value::Null));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_takes_a_rough_due_time_by_its_label() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli.run(&["item", "add", "Soon", "--due", "Next-Week", "--quiet"]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    let item = server.read_item(&ada, HEAP, added.stdout.trim()).await?;
    expect_that!(item["due"]["firm"], eq(false));
    expect_that!(item["due"]["label"], eq("next week"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_with_an_unknown_due_time_lists_the_choices_and_adds_nothing() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let added = cli.run(&["item", "add", "Never", "--due", "someday"]).await;

    expect_that!(added.exited(3), eq(true));
    expect_that!(added.stdout, eq(""));
    expect_that!(
        added.stderr,
        eq(
            "error: `someday` isn't a due time: give a date, YYYY-MM-DD, or one of: in a few days, next week, \
             in a few weeks, next month, in a few months\n"
        )
    );
    expect_that!(item_count(&server, &ada).await?, eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_with_an_unknown_tag_lists_the_heaps_tags_and_adds_nothing() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let added = cli.run(&["item", "add", "Never", "--tag", "Nope"]).await;

    expect_that!(added.exited(3), eq(true));
    expect_that!(
        added.stderr,
        eq(
            "error: ada/personal has no tag `Nope`; it has Infra\nhint: add it with `upheap tag add Nope --heap ada/personal`\n"
        )
    );
    expect_that!(item_count(&server, &ada).await?, eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_quiet_is_the_key_and_json_is_the_created_item() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let quiet = cli.run(&["item", "add", "Quietly", "-q"]).await;
    let json = cli.run(&["item", "add", "Loudly", "--json"]).await;

    // Assert
    let quiet_key = quiet.stdout.strip_suffix('\n').context("a line")?;
    expect_that!(quiet_key.len(), eq(5));
    expect_that!(server.read_item(&ada, HEAP, quiet_key).await?["title"], eq("Quietly"));
    let created: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(created["title"], eq("Loudly"));
    let created_key = created["key"].as_str().context("a key")?;
    expect_that!(
        server.read_item(&ada, HEAP, created_key).await?["id"],
        eq(&created["id"])
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_reads_a_description_from_stdin_without_its_last_newline() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    cli.stdin = b"line one\nline two\n".to_vec();

    // Act
    let added = cli.run(&["item", "add", "Noted", "--description", "-", "-q"]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    let item = server.read_item(&ada, HEAP, added.stdout.trim()).await?;
    expect_that!(item["description"], eq("line one\nline two"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_refuses_stdin_that_isnt_text() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    cli.stdin = vec![0xff, 0xfe, b'\n'];

    let added = cli.run(&["item", "add", "Garbled", "--description", "-"]).await;

    expect_that!(added.exited(2), eq(true));
    expect_that!(
        added.stderr,
        starts_with("error: can't read stdin as text for --description -")
    );
    expect_that!(item_count(&server, &ada).await?, eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_sends_an_empty_title_and_the_server_refuses_it() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let added = cli.run(&["item", "add", ""]).await;

    expect_that!(added.exited(3), eq(true));
    expect_that!(item_count(&server, &ada).await?, eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_needs_a_heap_and_refuses_json_with_quiet() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = common::TestCli::signed_in(&server, &ada).await?;

    let no_heap = cli.run(&["item", "add", "Where?"]).await;
    let both = cli.run(&["item", "add", "Both", "--json", "--quiet"]).await;

    expect_that!(no_heap.exited(2), eq(true));
    expect_that!(both.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_sets_a_parameter_by_name_ignoring_case() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;

    let added = cli
        .run(&["item", "add", "Fix the API", "--parameter", "area=backend", "-q"])
        .await;

    expect_that!(added.exited(0), eq(true));
    let item = server.read_item(&ada, common::HEAP, added.stdout.trim()).await?;
    expect_that!(item["parameters"][0]["name"], eq("Area"));
    expect_that!(item["parameters"][0]["option"]["label"], eq("Backend"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_add_with_an_unknown_option_lists_the_parameters_options_and_adds_nothing() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;

    let added = cli.run(&["item", "add", "Never", "--parameter", "Area=Mobile"]).await;
    let malformed = cli.run(&["item", "add", "Never", "--parameter", "Area"]).await;

    expect_that!(added.exited(3), eq(true));
    expect_that!(
        added.stderr,
        eq("error: parameter `Area` in ada/personal has no option `Mobile`; it has Backend, Frontend\n")
    );
    expect_that!(malformed.exited(2), eq(true));
    expect_that!(
        malformed.stderr,
        contains_substring("--parameter takes NAME=OPTION, such as --parameter Area=Backend; got `Area`")
    );
    expect_that!(item_count(&server, &ada).await?, eq(3));
    Ok(())
}
