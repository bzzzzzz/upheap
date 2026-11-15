//! `upheap work log`: what the agents reached, and the cursor for older runs.

mod common;

use anyhow::Result;
use common::{HEAP, TestServer, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

#[gtest]
#[tokio::test]
async fn a_user_whose_agents_reached_nothing_is_told_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let log = cli.run(&["work", "log"]).await;

    // Assert
    expect_that!(log.exited(0), eq(true));
    expect_that!(log.stdout, eq("No agent run reached anything in the last 30 days.\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reached_run_shows_its_result_and_links() -> Result<()> {
    // Arrange: a prepared note, taken by a run that reaches with context.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let heap = server.send(&ada, "GET", "/teams/ada/heaps/personal", None).await?;
    server
        .send_with(
            &ada,
            "PATCH",
            "/teams/ada/heaps/personal",
            Some(json!({ "agents_prepare": true })),
            &[("if-match", &heap["version"].to_string())],
        )
        .await?;
    let note = server
        .add_item(&ada, HEAP, json!({ "title": "Rotate the certs" }))
        .await?;
    let note = key(&note)?.to_owned();
    let mut cli = cli_in_heap(&server, &ada).await?;
    let dir = tempfile::tempdir()?;
    let context = dir.path().join("context.md");
    std::fs::write(&context, "The certs expire on Friday.\n")?;
    let context = context.to_string_lossy().into_owned();
    let claim = cli.run(&["work", "claim", "--quiet"]).await;
    cli.env.run = Some(claim.stdout.trim().to_owned());
    cli.run(&["item", "edit", &note, "--context-file", &context, "-q"])
        .await;
    cli.run(&[
        "work",
        "reached",
        "--result",
        "Wrote the context\nand the steps",
        "--link",
        "https://github.com/acme/app/pull/7",
        "--link",
        "https://wiki.example/page",
    ])
    .await;
    cli.run(&["work", "finish"]).await;
    cli.env.run = None;

    // Act
    let log = cli.run(&["work", "log"]).await;
    let quiet = cli.run(&["work", "log", "--quiet"]).await;
    let json = cli.run(&["work", "log", "--json"]).await;

    // Assert
    expect_that!(log.exited(0), eq(true));
    expect_that!(log.stdout, contains_substring("Today\n"));
    expect_that!(log.stdout, contains_substring(note.as_str()));
    expect_that!(log.stdout, contains_substring("Rotate the certs"));
    expect_that!(log.stdout, contains_substring("Personal"));
    expect_that!(log.stdout, contains_substring("with context"));
    expect_that!(
        log.stdout,
        contains_substring("\n    Wrote the context\n    and the steps\n")
    );
    expect_that!(
        log.stdout,
        contains_substring("\n    pull request https://github.com/acme/app/pull/7\n")
    );
    expect_that!(log.stdout, contains_substring("\n    https://wiki.example/page\n"));
    expect_that!(log.stdout, not(contains_substring("Older runs")));
    expect_that!(quiet.stdout, eq(&format!("ada/personal/{note}\n")));
    expect_that!(serde_json::from_str::<Value>(&json.stdout).is_ok(), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_cursor_the_server_did_not_issue_is_refused() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let log = cli.run(&["work", "log", "--cursor", "nonsense"]).await;

    // Assert
    expect_that!(log.exited(3), eq(true));
    expect_that!(log.stderr, contains_substring("cursor is not one this server issued"));
    Ok(())
}
