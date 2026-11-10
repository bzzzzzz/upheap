//! `upheap heap list`, `upheap heap use`, `upheap heap show` and `upheap heap edit`.

mod common;

use anyhow::Result;
use common::{TestCli, TestServer, TestUser};
use googletest::prelude::*;
use serde_json::json;

/// Ada with her personal team, and an `infra` team with heaps `backlog` and `ops`; her personal team has a
/// `backlog` too, and an archived `old`.
async fn ada_with_teams(server: &TestServer) -> Result<TestUser> {
    let ada = server.register("ada", "Ada").await?;
    server
        .send(
            &ada,
            "POST",
            "/teams",
            Some(json!({ "name": "Infrastructure", "slug": "infra" })),
        )
        .await?;
    for (team, slug) in [
        ("infra", "backlog"),
        ("infra", "ops"),
        ("ada", "backlog"),
        ("ada", "old"),
    ] {
        server
            .send(
                &ada,
                "POST",
                &format!("/teams/{team}/heaps"),
                Some(json!({ "name": slug, "slug": slug })),
            )
            .await?;
    }
    server.send(&ada, "POST", "/teams/ada/heaps/old/archive", None).await?;
    Ok(ada)
}

#[gtest]
#[tokio::test]
async fn heap_list_shows_each_team_with_its_heaps() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    server
        .add_item(&ada, "infra/ops", json!({ "title": "Patch the kernel" }))
        .await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let human = cli.run(&["heap", "list"]).await;
    let quiet = cli.run(&["heap", "list", "-q"]).await;
    let json = cli.run(&["heap", "list", "--json"]).await;

    // Assert
    expect_that!(
        human.stdout,
        eq("Ada (ada, personal)\n  \
              ada/personal  Personal  0 open\n  \
              ada/backlog   backlog   0 open\n  \
              ada/old       old       0 open  archived\n\
            \n\
            Infrastructure (infra)\n  \
              infra/backlog  backlog  0 open\n  \
              infra/ops      ops      1 open\n")
    );
    expect_that!(
        quiet.stdout,
        eq("ada/personal\nada/backlog\nada/old\ninfra/backlog\ninfra/ops\n")
    );
    let me: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(me["user"]["username"].as_str(), some(eq("ada")));
    expect_that!(json.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_sets_the_default_which_heap_list_marks() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let used = cli.run(&["heap", "use", "ops"]).await;
    let listed = cli.run(&["heap", "list"]).await;

    // Assert
    expect_that!(
        used.stdout,
        // The server came from UPHEAP_SERVER and the file had none, so it's saved with the heap.
        eq(&format!(
            "Commands now work in infra/ops unless told otherwise.\nThe default server is now {} too, the server infra/ops is on.\n",
            server.url
        ))
    );
    expect_that!(used.exited(0), eq(true));
    expect_that!(
        cli.config_text(),
        contains_substring(format!("server = \"{}\"", server.url))
    );
    expect_that!(cli.config_text(), contains_substring("heap = \"infra/ops\""));
    expect_that!(listed.stdout, contains_substring("* infra/ops      ops      0 open"));
    expect_that!(listed.stdout, contains_substring("  infra/backlog  backlog  0 open"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_refuses_a_slug_two_teams_have() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    let used = cli.run(&["heap", "use", "backlog"]).await;

    expect_that!(
        used.stderr,
        eq("error: more than one of your teams has a heap `backlog`: say which with ada/backlog or infra/backlog\n")
    );
    expect_that!(used.exited(3), eq(true));
    expect_that!(cli.config_text(), eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_refuses_a_heap_the_caller_doesnt_have() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    let unknown = cli.run(&["heap", "use", "infra/nope"]).await;
    let malformed = cli.run(&["heap", "use", "a/b/c"]).await;

    expect_that!(unknown.stderr, contains_substring("you have no heap `infra/nope`"));
    expect_that!(unknown.exited(3), eq(true));
    expect_that!(malformed.exited(2), eq(true));
    expect_that!(cli.config_text(), eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_with_another_server_saves_the_server_with_the_heap() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    cli.env.server = None;

    let used = cli.run(&["heap", "use", "ops", "--server", &server.url]).await;
    let again = cli.run(&["heap", "use", "ada/personal", "--server", &server.url]).await;

    expect_that!(
        used.stdout,
        eq(&format!(
            "Commands now work in infra/ops unless told otherwise.\nThe default server is now {} too, the server infra/ops is on.\n",
            server.url
        ))
    );
    expect_that!(
        again.stdout,
        eq("Commands now work in ada/personal unless told otherwise.\n")
    );
    expect_that!(
        cli.config_text(),
        contains_substring(format!("server = \"{}\"", server.url))
    );
    expect_that!(cli.config_text(), contains_substring("heap = \"ada/personal\""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_show_prints_the_context_indented_or_how_to_write_one() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    cli.env.heap = Some("ada/personal".to_owned());
    let ops = server.send(&ada, "GET", "/teams/infra/heaps/ops", None).await?;
    server
        .send_with(
            &ada,
            "PATCH",
            "/teams/infra/heaps/ops",
            Some(json!({ "context": "Docs: the wiki\r\n\tReview on GitHub" })),
            &[("if-match", &ops["version"].to_string())],
        )
        .await?;

    // Act
    let written = cli.run(&["heap", "show", "infra/ops"]).await;
    let empty = cli.run(&["heap", "show", "infra/backlog"]).await;
    let default = cli.run(&["heap", "show"]).await;
    let archived = cli.run(&["heap", "show", "ada/old"]).await;
    let json = cli.run(&["heap", "show", "ada/old", "--json"]).await;
    let quiet = cli.run(&["heap", "show", "ada/old", "-q"]).await;
    let unknown = cli.run(&["heap", "show", "infra/nope"]).await;

    // Assert
    expect_that!(
        written.stdout,
        eq("ops · Infrastructure\n\nContext\n  Docs: the wiki\n      Review on GitHub\n")
    );
    expect_that!(
        empty.stdout,
        eq(
            "backlog · Infrastructure\n\nContext\n  None. Write one with: upheap heap edit infra/backlog --context-file <file>\n"
        )
    );
    expect_that!(default.stdout, starts_with("Personal · Ada\n\nContext\n"));
    expect_that!(archived.stdout, starts_with("old · Ada · archived\n"));
    let heap: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(heap["slug"].as_str(), some(eq("old")));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(unknown.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_edit_writes_the_context_from_a_file_or_stdin_and_clears_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("context.txt");
    std::fs::write(&path, "Docs: the wiki\r\n\tReview on GitHub\n")?;
    let path = path.to_string_lossy().into_owned();

    // Act
    let from_file = cli.run(&["heap", "edit", "infra/ops", "--context-file", &path]).await;
    let shown = cli.run(&["heap", "show", "infra/ops"]).await;
    let again = cli.run(&["heap", "edit", "infra/ops", "--context-file", &path]).await;
    cli.stdin = b"From stdin\n".to_vec();
    let from_stdin = cli
        .run(&["heap", "edit", "infra/ops", "--context-file", "-", "--json"])
        .await;
    let cleared = cli.run(&["heap", "edit", "infra/ops", "--no-context"]).await;
    let quiet = cli
        .run(&["heap", "edit", "infra/ops", "--context-file", &path, "-q"])
        .await;
    let stored = server.send(&ada, "GET", "/teams/infra/heaps/ops", None).await?;

    // Assert
    expect_that!(from_file.stdout, eq("Updated the context of infra/ops.\n"));
    expect_that!(
        shown.stdout,
        eq("ops · Infrastructure\n\nContext\n  Docs: the wiki\n      Review on GitHub\n")
    );
    expect_that!(again.stdout, eq("No change to infra/ops.\n"));
    let heap: serde_json::Value = serde_json::from_str(&from_stdin.stdout)?;
    expect_that!(heap["context"].as_str(), some(eq("From stdin")));
    expect_that!(cleared.stdout, eq("Updated the context of infra/ops.\n"));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(
        stored["context"].as_str(),
        some(eq("Docs: the wiki\r\n\tReview on GitHub"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_edit_does_nothing_without_a_flag_and_refuses_both_or_an_unreadable_file() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let none = cli.run(&["heap", "edit", "infra/ops"]).await;
    let both = cli
        .run(&["heap", "edit", "infra/ops", "--context-file", "x.txt", "--no-context"])
        .await;
    let missing = cli
        .run(&[
            "heap",
            "edit",
            "infra/ops",
            "--context-file",
            "/nonexistent/context.txt",
        ])
        .await;
    let archived = cli.run(&["heap", "edit", "ada/old", "--no-context"]).await;

    // Assert
    expect_that!(none.exited(0), eq(true));
    expect_that!(
        none.stdout,
        eq("Nothing to change: give --context-file or --no-context.\n")
    );
    expect_that!(both.exited(2), eq(true));
    expect_that!(missing.exited(2), eq(true));
    expect_that!(missing.stderr, contains_substring("/nonexistent/context.txt"));
    expect_that!(archived.exited(0), eq(false));
    expect_that!(archived.stderr, contains_substring("archived"));
    Ok(())
}
