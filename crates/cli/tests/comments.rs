//! `upheap comment add | list` and the Comments section of `upheap item show`.

mod common;

use anyhow::Result;
use common::{HEAP, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::Value;

/// A server with Ada and her items. Returns the first item's key too.
async fn with_item() -> Result<(TestServer, common::TestUser, String)> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let item = key(&items[0])?.to_owned();
    Ok((server, ada, item))
}

#[gtest]
#[tokio::test]
async fn comment_add_says_what_it_added_and_prints_its_name_with_quiet() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli.run(&["comment", "add", &item, "Use the internal CA."]).await;
    let quiet = cli.run(&["comment", "add", &item, "Installed it.", "--quiet"]).await;
    let json = cli.run(&["comment", "add", &item, "One more", "--json"]).await;

    // Assert
    expect_that!(human.stdout, eq(&format!("Commented on {item} ({item}.c1).\n")));
    expect_that!(human.stderr, eq(""));
    expect_that!(human.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(&format!("{item}.c2\n")));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["comments"][2]["body"], eq("One more"));
    expect_that!(body["comments"][2]["number"], eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn comment_add_reads_the_text_from_a_file_or_stdin() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("comment.md");
    std::fs::write(
        &path,
        "Installed the new certificate.\nProduction still has the old one.\n",
    )?;
    let path = path.to_string_lossy().into_owned();

    // Act
    let from_file = cli.run(&["comment", "add", &item, "--file", &path, "-q"]).await;
    cli.stdin = b"From stdin\nsecond line\n".to_vec();
    let from_stdin = cli.run(&["comment", "add", &item, "--file", "-", "-q"]).await;

    // Assert
    expect_that!(from_file.stdout, eq(&format!("{item}.c1\n")));
    expect_that!(from_stdin.stdout, eq(&format!("{item}.c2\n")));
    let read = server.read_item(&ada, HEAP, &item).await?;
    expect_that!(
        read["comments"][0]["body"],
        eq("Installed the new certificate.\nProduction still has the old one.")
    );
    expect_that!(read["comments"][1]["body"], eq("From stdin\nsecond line"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn comment_add_needs_exactly_one_of_text_and_file_and_sends_nothing_otherwise() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let neither = cli.run(&["comment", "add", &item]).await;
    let both = cli
        .run(&["comment", "add", &item, "Some text", "--file", "comment.md"])
        .await;

    // Assert
    expect_that!(neither.exited(2), eq(true));
    expect_that!(both.exited(2), eq(true));
    let read = server.read_item(&ada, HEAP, &item).await?;
    expect_that!(read["comments"].as_array().map(Vec::len), some(eq(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blank_comment_is_the_servers_refusal_and_an_unknown_item_points_to_the_list() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let blank = cli.run(&["comment", "add", &item, "   "]).await;
    let unknown = cli.run(&["comment", "add", "zzzzz", "Anyone?"]).await;

    // Assert
    expect_that!(blank.exited(3), eq(true));
    expect_that!(blank.stderr, contains_substring("body"));
    expect_that!(unknown.exited(4), eq(true));
    expect_that!(unknown.stderr, contains_substring("upheap item list"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_takes_a_comment() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let fresh = server.read_item(&ada, HEAP, &item).await?;
    server.set_state(&ada, HEAP, &fresh, "done").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli.run(&["comment", "add", &item, "Rotated on Friday."]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stdout, eq(&format!("Commented on {item} ({item}.c1).\n")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_201st_comment_is_the_servers_refusal() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    for n in 1..=200 {
        server
            .send(
                &ada,
                "POST",
                &format!("/teams/ada/heaps/personal/items/{item}/comments"),
                Some(serde_json::json!({ "body": format!("Comment {n}") })),
            )
            .await?;
    }
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let refused = cli.run(&["comment", "add", &item, "One more"]).await;

    // Assert
    expect_that!(refused.exited(3), eq(true));
    expect_that!(refused.stderr, contains_substring("too_many_comments"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn comment_list_prints_the_thread_oldest_first_with_bodies_indented() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["comment", "add", &item, "Use the internal CA\nfor staging."])
        .await;
    cli.run(&["comment", "add", &item, "Tab:\there\n\nafter a blank line"])
        .await;

    // Act
    let human = cli.run(&["comment", "list", &item]).await;
    let quiet = cli.run(&["comment", "list", &item, "-q"]).await;
    let json = cli.run(&["comment", "list", &item, "--json"]).await;

    // Assert
    let lines: Vec<&str> = human.stdout.lines().collect();
    expect_that!(lines.len(), eq(7));
    expect_that!(lines[0], starts_with("c1  Ada, "));
    expect_that!(lines[1], eq("    Use the internal CA"));
    expect_that!(lines[2], eq("    for staging."));
    expect_that!(lines[3], starts_with("c2  Ada, "));
    expect_that!(lines[4], eq("    Tab:    here"));
    expect_that!(lines[5], eq(""));
    expect_that!(lines[6], eq("    after a blank line"));
    expect_that!(human.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(&format!("{item}.c1\n{item}.c2\n")));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["key"], eq(&item));
    expect_that!(body["comments"][1]["number"], eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn comment_list_says_so_when_there_are_none() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli.run(&["comment", "list", &item]).await;
    let quiet = cli.run(&["comment", "list", &item, "-q"]).await;

    // Assert
    expect_that!(human.stdout, eq(&format!("No comments on {item}.\n")));
    expect_that!(human.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(quiet.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_prints_the_comments_after_the_context_and_before_the_questions() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["comment", "add", &item, "Use the internal CA."]).await;
    cli.run(&[
        "comment",
        "add",
        &item,
        "Installed it.\nProduction still has the old one.",
    ])
    .await;
    cli.run(&["question", "ask", &item, "Which CA?"]).await;

    // Act
    let shown = cli.run(&["item", "show", &item]).await;

    // Assert
    let lines: Vec<&str> = shown.stdout.lines().collect();
    let heading = lines.iter().position(|line| *line == "Comments");
    let context = lines.iter().position(|line| *line == "Context");
    let questions = lines.iter().position(|line| *line == "Questions");
    let (Some(heading), Some(context), Some(questions)) = (heading, context, questions) else {
        return Err(anyhow::anyhow!("a section is missing from:\n{}", shown.stdout));
    };
    expect_that!(context < heading, eq(true));
    expect_that!(heading < questions, eq(true));
    expect_that!(lines[heading + 1], starts_with("c1  Ada, "));
    expect_that!(lines[heading + 2], eq("    Use the internal CA."));
    expect_that!(lines[heading + 3], starts_with("c2  Ada, "));
    expect_that!(lines[heading + 4], eq("    Installed it."));
    expect_that!(lines[heading + 5], eq("    Production still has the old one."));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_has_no_comments_section_for_an_item_without_comments() -> Result<()> {
    // Arrange
    let (server, ada, item) = with_item().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let shown = cli.run(&["item", "show", &item]).await;

    // Assert
    expect_that!(shown.stdout, not(contains_substring("Comments")));
    Ok(())
}
