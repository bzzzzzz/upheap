//! `upheap heap list` and `upheap heap use`.

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
        eq("Commands now work in infra/ops unless told otherwise.\n")
    );
    expect_that!(used.exited(0), eq(true));
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
