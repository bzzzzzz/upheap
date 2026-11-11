//! `upheap work next | claim | reached | finish`, the writes an agent makes in a run, and what a run is refused.

mod common;

use anyhow::{Context, Result};
use common::{HEAP, TestCli, TestServer, TestUser, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

/// A server with Ada and a heap that prepares every new note, with a note added in it. Returns the note's key.
async fn with_prepared_note() -> Result<(TestServer, TestUser, TestCli, String)> {
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
    let cli = cli_in_heap(&server, &ada).await?;
    Ok((server, ada, cli, note))
}

#[gtest]
#[tokio::test]
async fn a_script_that_plays_the_agent_takes_a_note_to_with_context() -> Result<()> {
    // Arrange: the heap prepares the note, so it is in the queue at once.
    let (server, ada, mut cli, note) = with_prepared_note().await?;
    let dir = tempfile::tempdir()?;
    let context = dir.path().join("context.md");
    std::fs::write(&context, "The certs expire on Friday.\n")?;
    let context = context.to_string_lossy().into_owned();

    // Act
    let next = cli.run(&["work", "next"]).await;
    let next_quiet = cli.run(&["work", "next", "--quiet"]).await;
    let claim = cli.run(&["work", "claim", "--quiet"]).await;
    let run = claim.stdout.trim().to_owned();
    cli.env.run = Some(run.clone());
    let edited = cli
        .run(&["item", "edit", &note, "--context-file", &context, "-q"])
        .await;
    let reached = cli
        .run(&[
            "work",
            "reached",
            "--result",
            "Wrote the context",
            "--link",
            "https://example.com/pr/1",
        ])
        .await;
    let finished = cli.run(&["work", "finish", "--cost-usd", "0.12"]).await;
    cli.env.run = None;
    let shown = cli.run(&["item", "show", &note]).await;
    let item = server.read_item(&ada, HEAP, &note).await?;
    let empty = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(next.exited(0), eq(true));
    expect_that!(next.stdout, contains_substring(note.as_str()));
    expect_that!(next.stdout, contains_substring("Rotate the certs"));
    expect_that!(next.stdout, contains_substring("Personal"));
    expect_that!(next.stdout, contains_substring("note → with context"));
    expect_that!(next.stdout, not(contains_substring("started")));
    expect_that!(next_quiet.stdout, eq(&format!("{note}\n")));
    expect_that!(claim.exited(0), eq(true));
    expect_that!(uuid::Uuid::parse_str(&run).is_ok(), eq(true));
    expect_that!(edited.exited(0), eq(true));
    expect_that!(reached.exited(0), eq(true));
    expect_that!(
        reached.stdout,
        eq(&format!("Reached: with context. {note} is yours again.\n"))
    );
    expect_that!(finished.exited(0), eq(true));
    expect_that!(finished.stdout, eq("Run ended: reached. ($0.12)\n"));
    expect_that!(shown.stdout, contains_substring("with context"));
    expect_that!(shown.stdout, not(contains_substring("with an agent")));
    expect_that!(shown.stdout, not(contains_substring("Turn:")));
    expect_that!(item["stage"], eq("with_context"));
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(empty.stdout, eq("Nothing to do.\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn claim_prints_the_run_and_how_to_use_it() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, note) = with_prepared_note().await?;

    // Act
    let claim = cli.run(&["work", "claim"]).await;
    let none = cli.run(&["work", "claim"]).await;
    let none_quiet = cli.run(&["work", "claim", "--quiet"]).await;

    // Assert
    expect_that!(claim.exited(0), eq(true));
    let lines: Vec<&str> = claim.stdout.lines().collect();
    expect_that!(lines.len(), eq(2));
    expect_that!(lines[0], starts_with("Run "));
    expect_that!(lines[0], contains_substring(note.as_str()));
    expect_that!(lines[0], contains_substring("note → with context"));
    let id = lines[0].split_whitespace().nth(1).context("run id")?;
    expect_that!(lines[1], eq(&format!("export UPHEAP_RUN={id}")));
    expect_that!(none.exited(0), eq(true));
    expect_that!(none.stdout, eq("Nothing to do.\n"));
    expect_that!(none_quiet.exited(0), eq(true));
    expect_that!(none_quiet.stdout, eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn next_json_prints_the_servers_answer_and_a_heap_flag_names_the_heap() -> Result<()> {
    // Arrange
    let (_server, _ada, mut cli, note) = with_prepared_note().await?;
    cli.env.heap = None;

    // Act
    let no_default = cli.run(&["work", "next"]).await;
    let json = cli.run(&["work", "next", "--heap", "ada/personal", "--json"]).await;
    let by_slug = cli
        .run(&["work", "next", "--heap", "personal", "--heap", "ada/personal", "-q"])
        .await;
    let unknown = cli.run(&["work", "next", "--heap", "nowhere"]).await;

    // Assert
    expect_that!(no_default.exited(2), eq(true));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["item"]["key"], eq(&json!(note)));
    expect_that!(body["waiting_count"], eq(&json!(0)));
    expect_that!(by_slug.stdout, eq(&format!("{note}\n")));
    expect_that!(unknown.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn next_says_how_many_items_wait_for_you() -> Result<()> {
    // Arrange: the only item has an open question.
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let asked = cli.run(&["question", "ask", &note, "Which CA?"]).await;

    // Act
    let next = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(asked.exited(0), eq(true));
    expect_that!(next.stdout, eq("Nothing to do. 1 item waits for you.\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn claim_inside_a_run_and_reached_or_finish_outside_one_are_usage_errors() -> Result<()> {
    // Arrange
    let (_server, _ada, mut cli, _note) = with_prepared_note().await?;
    let claimed = cli.run(&["work", "claim", "--quiet"]).await;
    let run = claimed.stdout.trim().to_owned();

    // Act
    let reached_out = cli.run(&["work", "reached", "--result", "x"]).await;
    let finish_out = cli.run(&["work", "finish"]).await;
    cli.env.run = Some(run);
    let claim_in = cli.run(&["work", "claim"]).await;
    cli.env.run = Some("not-a-run".to_owned());
    let bad = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(reached_out.exited(2), eq(true));
    expect_that!(
        reached_out.stderr,
        contains_substring("work reached runs inside a run: set UPHEAP_RUN")
    );
    expect_that!(finish_out.exited(2), eq(true));
    expect_that!(
        finish_out.stderr,
        contains_substring("work finish runs inside a run: set UPHEAP_RUN")
    );
    expect_that!(claim_in.exited(2), eq(true));
    expect_that!(
        claim_in.stderr,
        contains_substring("work claim starts a run; unset UPHEAP_RUN first")
    );
    expect_that!(bad.exited(2), eq(true));
    expect_that!(bad.stderr, contains_substring("UPHEAP_RUN isn't a run ID"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn what_a_run_is_refused_is_exit_3_with_the_servers_message_and_a_hint() -> Result<()> {
    // Arrange: a run on the prepared note, and a second item it doesn't hold.
    let (server, ada, mut cli, note) = with_prepared_note().await?;
    let other = server.add_item(&ada, HEAP, json!({ "title": "Tidy the wiki" })).await?;
    let other = key(&other)?.to_owned();
    // The other item must rank below the prepared note to leave the claim to it: it has no turn, so it can't be claimed.
    let claimed = cli.run(&["work", "claim", "--quiet"]).await;
    cli.env.run = Some(claimed.stdout.trim().to_owned());

    // Act
    let title = cli.run(&["item", "edit", &note, "--title", "Other"]).await;
    let wrong_item = cli.run(&["item", "edit", &other, "--description", "Not mine"]).await;
    let done = cli.run(&["item", "done", &note]).await;
    let not_reached = cli.run(&["work", "reached", "--result", "Did it"]).await;
    let finished = cli.run(&["work", "finish"]).await;
    let over = cli.run(&["item", "edit", &note, "--description", "Too late"]).await;

    // Assert
    expect_that!(title.exited(3), eq(true));
    expect_that!(title.stderr, contains_substring("(run_forbidden)"));
    expect_that!(title.stderr, contains_substring("hint: Only the person does that."));
    expect_that!(wrong_item.exited(3), eq(true));
    expect_that!(wrong_item.stderr, contains_substring("(run_wrong_item)"));
    expect_that!(
        wrong_item.stderr,
        contains_substring(format!("hint: This run holds {note}; write to it only.").as_str())
    );
    expect_that!(done.exited(3), eq(true));
    expect_that!(done.stderr, contains_substring("(run_forbidden)"));
    expect_that!(not_reached.exited(3), eq(true));
    expect_that!(
        not_reached.stderr,
        contains_substring("The item has no context yet. (not_reached)")
    );
    expect_that!(
        not_reached.stderr,
        contains_substring("hint: Fix what's missing and say reached again.")
    );
    expect_that!(finished.exited(0), eq(true));
    expect_that!(finished.stdout.starts_with("Run ended: failed: "), eq(true));
    expect_that!(over.exited(3), eq(true));
    expect_that!(over.stderr, contains_substring("(run_over)"));
    expect_that!(over.stderr, contains_substring("hint: This run is over. Stop."));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn three_failed_runs_make_item_show_say_the_turn_is_stuck() -> Result<()> {
    // Arrange
    let (_server, _ada, mut cli, note) = with_prepared_note().await?;
    let mut finished = Vec::new();

    // Act
    for _ in 0..3 {
        let claimed = cli.run(&["work", "claim", "--quiet"]).await;
        cli.env.run = Some(claimed.stdout.trim().to_owned());
        finished.push(cli.run(&["work", "finish", "--failure", "The tool crashed"]).await);
        cli.env.run = None;
    }
    let shown = cli.run(&["item", "show", &note]).await;
    let next = cli.run(&["work", "next"]).await;
    let gone = cli.run(&["work", "claim", "--quiet"]).await;

    // Assert
    expect_that!(finished[0].stdout, eq("Run ended: failed: The tool crashed.\n"));
    expect_that!(
        shown.stdout,
        contains_substring("Runs: 3. Stuck: 3 runs in a row failed.")
    );
    expect_that!(shown.stdout, contains_substring("\n  The tool crashed\n"));
    expect_that!(next.stdout, eq("Nothing to do. 1 item waits for you.\n"));
    expect_that!(gone.stdout, eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_next_item_that_was_started_says_so() -> Result<()> {
    // Arrange: one failed run, so the turn has had a run.
    let (_server, _ada, mut cli, _note) = with_prepared_note().await?;
    let claimed = cli.run(&["work", "claim", "--quiet"]).await;
    cli.env.run = Some(claimed.stdout.trim().to_owned());
    cli.run(&["work", "finish"]).await;
    cli.env.run = None;

    // Act
    let next = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(next.stdout, contains_substring("started, ready to go on"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_run_header_is_only_sent_when_upheap_run_is_set() -> Result<()> {
    // Arrange: ordinary commands keep working without a run, and with one they are held to it.
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    let first = key(&items[0])?;

    // Act
    let plain = cli.run(&["item", "edit", first, "--title", "Renamed", "-q"]).await;
    cli.env.run = Some("018f0000-0000-7000-8000-000000000000".to_owned());
    let in_run = cli
        .run(&["item", "edit", first, "--description", "Another", "-q"])
        .await;
    let read = cli.run(&["item", "show", first]).await;

    // Assert
    expect_that!(plain.exited(0), eq(true));
    // A run nobody has made is not the caller's: the server says it isn't there.
    expect_that!(in_run.exited(4), eq(true));
    expect_that!(read.exited(0), eq(true));
    Ok(())
}
