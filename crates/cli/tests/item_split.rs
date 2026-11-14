//! Splitting a note: `upheap item add --from`, `upheap item became`, `upheap item to-step`, and the From and Became
//! lines of `item show`.

mod common;

use anyhow::Result;
use common::{HEAP, TestCli, TestServer, TestUser, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

/// A server with Ada, whose note "Certs are a mess" has a context, and the CLI signed in as her.
async fn with_a_note() -> Result<(TestServer, TestUser, TestCli, String)> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let note = server
        .add_item(
            &ada,
            HEAP,
            json!({ "title": "Certs are a mess", "need": "expected", "description": "Two kinds of certs." }),
        )
        .await?;
    let note = key(&note)?.to_owned();
    let cli = cli_in_heap(&server, &ada).await?;
    Ok((server, ada, cli, note))
}

#[gtest]
#[tokio::test]
async fn from_needs_a_need_and_an_age_and_age_needs_from_before_any_request() -> Result<()> {
    // Arrange
    let (server, ada, cli, note) = with_a_note().await?;

    // Act
    let no_need = cli
        .run(&["item", "add", "Rotate staging", "--from", &note, "--age", "keep"])
        .await;
    let no_age = cli
        .run(&["item", "add", "Rotate staging", "--from", &note, "--need", "expected"])
        .await;
    let age_alone = cli
        .run(&["item", "add", "Rotate staging", "--age", "keep", "--need", "expected"])
        .await;
    let reminder = cli
        .run(&[
            "item",
            "add",
            "Rotate staging",
            "--from",
            &note,
            "--need",
            "expected",
            "--age",
            "keep",
            "--reminder",
            "tomorrow",
        ])
        .await;

    // Assert
    for refused in [&no_need, &no_age, &age_alone, &reminder] {
        expect_that!(refused.exited(2), eq(true));
    }
    let listed = server
        .send(&ada, "GET", "/teams/ada/heaps/personal/items", None)
        .await?;
    expect_that!(listed["ranked"].as_array().map(Vec::len), some(eq(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_task_is_made_from_a_note_and_the_note_became_it() -> Result<()> {
    // Arrange
    let (server, ada, cli, note) = with_a_note().await?;
    let nothing_made = cli.run(&["item", "became", &note]).await;

    // Act
    let added = cli
        .run(&[
            "item",
            "add",
            "Rotate staging",
            "--task",
            "--from",
            &note,
            "--need",
            "expected",
            "--age",
            "keep",
            "--quiet",
        ])
        .await;
    let first = added.stdout.trim().to_owned();
    let second = cli
        .run(&[
            "item",
            "add",
            "Rotate production",
            "--task",
            "--from",
            &note,
            "--need",
            "critical",
            "--age",
            "new",
            "--blocked-by",
            &first,
            "--quiet",
        ])
        .await;
    let shown_note = cli.run(&["item", "show", &note]).await;
    let shown_task = cli.run(&["item", "show", &first]).await;
    let shown_second = cli.run(&["item", "show", second.stdout.trim()]).await;
    let became = cli.run(&["item", "became", &note]).await;
    let quiet = cli.run(&["item", "became", &note, "--quiet"]).await;
    let json = cli.run(&["item", "became", &note, "--json"]).await;

    // Assert
    expect_that!(nothing_made.exited(3), eq(true));
    expect_that!(nothing_made.stderr, contains_substring("(nothing_made)"));
    expect_that!(
        nothing_made.stderr,
        contains_substring("hint: Make the tasks first: upheap item add")
    );
    expect_that!(added.exited(0), eq(true));
    expect_that!(second.exited(0), eq(true));
    expect_that!(
        shown_note.stdout,
        contains_substring(format!("Became:\n  {first} \"Rotate staging\" (task, open)\n"))
    );
    expect_that!(
        shown_note.stdout,
        contains_substring(format!(
            "{} \"Rotate production\" (task, blocked)",
            second.stdout.trim()
        ))
    );
    expect_that!(
        shown_task.stdout,
        contains_substring(format!("From: {note} \"Certs are a mess\" (note, ada/personal)"))
    );
    expect_that!(
        shown_second.stdout,
        contains_substring(format!("Blocked by  {first}  Rotate staging (open)"))
    );
    expect_that!(
        became.stdout,
        eq(&format!("{note} \"Certs are a mess\" became 2 items.\n"))
    );
    expect_that!(quiet.stdout, eq(&format!("{note}\n")));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["state"], eq("became"));
    expect_that!(server.read_item(&ada, HEAP, &note).await?["state"], eq("became"));
    let shown = cli.run(&["item", "show", &note]).await;
    expect_that!(shown.stdout, contains_substring("became"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn in_a_run_the_note_became_ends_the_turn_and_a_need_above_the_notes_is_refused() -> Result<()> {
    // Arrange: a note sent to an agent, and a run on it.
    let (server, ada, mut cli, note) = with_a_note().await?;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{note}/turn"),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    let claim = cli.run(&["work", "claim", "--quiet"]).await;
    cli.env.run = Some(claim.stdout.trim().to_owned());

    // Act
    let too_high = cli
        .run(&[
            "item",
            "add",
            "Rotate staging",
            "--task",
            "--from",
            &note,
            "--need",
            "critical",
            "--age",
            "keep",
            "--description",
            "Done when: staging has new certs.",
        ])
        .await;
    let made = cli
        .run(&[
            "item",
            "add",
            "Rotate staging",
            "--task",
            "--from",
            &note,
            "--need",
            "expected",
            "--age",
            "keep",
            "--description",
            "Done when: staging has new certs.",
            "--step",
            "Rotate",
            "--quiet",
        ])
        .await;
    let became = cli.run(&["item", "became", &note]).await;

    // Assert
    expect_that!(too_high.exited(3), eq(true));
    expect_that!(too_high.stderr, contains_substring("(run_need_above_origin)"));
    expect_that!(
        too_high.stderr,
        contains_substring("hint: Give --need expected or lower.")
    );
    expect_that!(made.exited(0), eq(true));
    expect_that!(
        became.stdout,
        eq(&format!(
            "{note} \"Certs are a mess\" became 1 item.\nThe turn is reached.\n"
        ))
    );
    let task = server.read_item(&ada, HEAP, made.stdout.trim()).await?;
    expect_that!(task["stage"], eq("defined"));
    Ok(())
}

/// Adds the task "Rotate the certs" and returns its key.
async fn add_task(server: &TestServer, ada: &TestUser) -> Result<String> {
    let task = server
        .add_item(ada, HEAP, json!({ "title": "Rotate the certs", "kind": "task" }))
        .await?;
    Ok(key(&task)?.to_owned())
}

#[gtest]
#[tokio::test]
async fn a_note_becomes_a_step_of_a_task_and_item_show_lists_it() -> Result<()> {
    // Arrange
    let (server, ada, cli, note) = with_a_note().await?;
    let task = add_task(&server, &ada).await?;
    let other = server.add_item(&ada, HEAP, json!({ "title": "Another note" })).await?;
    let other = key(&other)?.to_owned();

    // Act
    let moved = cli.run(&["item", "to-step", &note, "--task", &task]).await;
    let quiet = cli.run(&["item", "to-step", &other, "--task", &task, "--quiet"]).await;
    let shown = cli.run(&["item", "show", &note]).await;
    let json = cli.run(&["item", "show", &note, "--json"]).await;

    // Assert
    expect_that!(moved.exited(0), eq(true));
    expect_that!(
        moved.stdout,
        eq(&format!(
            "{note} \"Certs are a mess\" became step {task}.1 of {task} \"Rotate the certs\".\n"
        ))
    );
    expect_that!(quiet.stdout, eq(&format!("{task}.2\n")));
    expect_that!(
        shown.stdout,
        contains_substring(format!(
            "Became:\n  step {task}.1 \"Certs are a mess\" of {task} \"Rotate the certs\" (open)\n"
        ))
    );
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["became_steps"][0]["task"]["key"], eq(task.as_str()));
    let step_note = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(step_note["steps"][0]["origin"]["key"], eq(note.as_str()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn in_a_run_the_note_to_step_reaches_the_turn() -> Result<()> {
    // Arrange
    let (server, ada, mut cli, note) = with_a_note().await?;
    let task = add_task(&server, &ada).await?;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{note}/turn"),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    let claim = cli.run(&["work", "claim", "--quiet"]).await;
    cli.env.run = Some(claim.stdout.trim().to_owned());

    // Act
    let moved = cli.run(&["item", "to-step", &note, "--task", &task]).await;

    // Assert
    expect_that!(
        moved.stdout,
        eq(&format!(
            "{note} \"Certs are a mess\" became step {task}.1 of {task} \"Rotate the certs\".\nThe turn is reached.\n"
        ))
    );
    Ok(())
}
