//! `upheap step add | tick | drop | edit` and the Steps section of `upheap item show`.

mod common;

use anyhow::Result;
use common::{HEAP, TestServer, TestUser, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

/// A server with Ada and her items, the first of which is a task. Returns the first item's key too.
async fn with_task() -> Result<(TestServer, TestUser, Vec<Value>, String)> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let task = key(&items[0])?.to_owned();
    server.edit_item(&ada, HEAP, &task, json!({ "kind": "task" })).await?;
    Ok((server, ada, items, task))
}

/// Adds a step to the task through the API.
async fn add_step(server: &TestServer, user: &TestUser, task: &str, title: &str) -> Result<Value> {
    server
        .send(
            user,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{task}/steps"),
            Some(json!({ "title": title })),
        )
        .await
}

#[gtest]
#[tokio::test]
async fn step_add_says_what_it_added_and_prints_its_name_with_quiet() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli.run(&["step", "add", &task, "Run in staging"]).await;
    let quiet = cli.run(&["step", "add", &task, "Email ops", "--quiet"]).await;
    let json = cli.run(&["step", "add", &task, "Wrap up", "--json"]).await;

    // Assert
    expect_that!(human.stdout, eq(&format!("Added {task}.1: Run in staging.\n")));
    expect_that!(human.stderr, eq(""));
    expect_that!(human.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(&format!("{task}.2\n")));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["steps"][2]["title"], eq("Wrap up"));
    expect_that!(body["steps"][2]["number"], eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn step_add_after_takes_a_step_name_or_a_number() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    add_step(&server, &ada, &task, "One").await?;
    add_step(&server, &ada, &task, "Two").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let by_name = cli
        .run(&["step", "add", &task, "Three", "--after", &format!("{task}.1")])
        .await;
    let by_number = cli.run(&["step", "add", &task, "Four", "--after", "1"]).await;

    // Assert
    expect_that!(by_name.exited(0), eq(true));
    expect_that!(by_number.exited(0), eq(true));
    let read = server.read_item(&ada, HEAP, &task).await?;
    let titles: Vec<&str> = read["steps"]
        .as_array()
        .map(|steps| steps.iter().filter_map(|step| step["title"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(titles, elements_are![eq(&"One"), eq(&"Four"), eq(&"Three"), eq(&"Two")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn step_add_after_a_step_of_another_item_is_a_usage_error() -> Result<()> {
    // Arrange
    let (server, ada, items, task) = with_task().await?;
    let other = key(&items[1])?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let refused = cli
        .run(&["step", "add", &task, "Three", "--after", &format!("{other}.1")])
        .await;
    let malformed = cli.run(&["step", "add", &task, "Three", "--after", "x"]).await;

    // Assert
    expect_that!(refused.exited(2), eq(true));
    expect_that!(refused.stderr, contains_substring("another item"));
    expect_that!(malformed.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn step_add_reads_the_context_from_a_file_or_stdin() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("context.txt");
    std::fs::write(&path, "Use the blue stack\n")?;
    let path = path.to_string_lossy().into_owned();

    // Act
    let from_file = cli
        .run(&["step", "add", &task, "From a file", "--context-file", &path])
        .await;
    cli.stdin = b"From stdin\nsecond line\n".to_vec();
    let from_stdin = cli
        .run(&["step", "add", &task, "From stdin", "--context-file", "-"])
        .await;
    let missing = cli
        .run(&[
            "step",
            "add",
            &task,
            "Missing",
            "--context-file",
            "/nonexistent/context.txt",
        ])
        .await;

    // Assert
    expect_that!(from_file.exited(0), eq(true));
    expect_that!(from_stdin.exited(0), eq(true));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["steps"][0]["context"], eq("Use the blue stack"));
    expect_that!(read["steps"][1]["context"], eq("From stdin\nsecond line"));
    expect_that!(missing.exited(2), eq(true));
    expect_that!(missing.stderr, contains_substring("/nonexistent/context.txt"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn step_tick_ticks_once_and_says_so_the_second_time() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    add_step(&server, &ada, &task, "Run in staging").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let name = format!("{task}.1");

    // Act
    let first = cli.run(&["step", "tick", &name]).await;
    let version = server.read_item(&ada, HEAP, &task).await?["steps"][0]["version"].clone();
    let again = cli.run(&["step", "tick", &name]).await;
    let quiet = cli.run(&["step", "tick", &name, "--quiet"]).await;
    let json = cli.run(&["step", "tick", &name, "--json"]).await;

    // Assert
    expect_that!(first.stdout, eq(&format!("Ticked {name}: Run in staging.\n")));
    expect_that!(first.exited(0), eq(true));
    expect_that!(again.stdout, eq(&format!("{name} is already ticked.\n")));
    expect_that!(again.exited(0), eq(true));
    expect_that!((quiet.stdout.as_str(), quiet.exited(0)), eq(("", true)));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["steps"][0]["state"], eq("ticked"));
    expect_that!(
        server.read_item(&ada, HEAP, &task).await?["steps"][0]["version"],
        eq(&version)
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn step_drop_drops_a_step_and_is_not_repeated() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    add_step(&server, &ada, &task, "Email ops").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let name = format!("{task}.1");

    // Act
    let dropped = cli.run(&["step", "drop", &name]).await;
    let again = cli.run(&["step", "drop", &name]).await;

    // Assert
    expect_that!(dropped.stdout, eq(&format!("Dropped {name}: Email ops.\n")));
    expect_that!(again.stdout, eq(&format!("{name} is already dropped.\n")));
    expect_that!(again.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn step_edit_changes_the_title_and_clears_the_context() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{task}/steps"),
            Some(json!({ "title": "Run in staging", "context": "Blue stack" })),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let name = format!("{task}.1");

    // Act
    let retitled = cli.run(&["step", "edit", &name, "--title", "Run in production"]).await;
    let cleared = cli.run(&["step", "edit", &name, "--no-context"]).await;
    let nothing = cli.run(&["step", "edit", &name]).await;
    let same = cli.run(&["step", "edit", &name, "--title", "Run in production"]).await;

    // Assert
    expect_that!(retitled.stdout, eq(&format!("Edited {name}: Run in production.\n")));
    expect_that!(cleared.exited(0), eq(true));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["steps"][0]["title"], eq("Run in production"));
    expect_that!(read["steps"][0]["context"], eq(&Value::Null));
    expect_that!(nothing.exited(0), eq(true));
    expect_that!(
        nothing.stdout,
        eq("Nothing to change: give --title, --context-file or --no-context.\n")
    );
    expect_that!(same.stdout, eq(&format!("No change to {name}.\n")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_name_that_is_not_a_step_is_a_usage_error_and_a_missing_step_is_not_found() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    add_step(&server, &ada, &task, "One").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let mut malformed = Vec::new();
    for bad in [task.clone(), format!("{task}.x"), format!("{task}.0")] {
        malformed.push(cli.run(&["step", "tick", &bad]).await);
    }
    let missing = cli.run(&["step", "tick", &format!("{task}.9")]).await;
    let missing_edit = cli.run(&["step", "edit", &format!("{task}.9"), "--title", "x"]).await;
    let no_item = cli.run(&["step", "tick", "zzzzz.1"]).await;

    // Assert
    for outcome in &malformed {
        expect_that!(outcome.exited(2), eq(true));
        expect_that!(outcome.stderr, contains_substring("isn't a step"));
    }
    expect_that!(missing.exited(4), eq(true));
    expect_that!(
        missing.stderr,
        starts_with(format!("error: no step `{task}.9` in {HEAP}"))
    );
    expect_that!(missing_edit.exited(4), eq(true));
    expect_that!(no_item.exited(4), eq(true));
    expect_that!(no_item.stderr, contains_substring("no item `zzzzz`"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn adding_to_a_note_and_changing_a_closed_task_are_refused_by_the_server() -> Result<()> {
    // Arrange
    let (server, ada, items, task) = with_task().await?;
    add_step(&server, &ada, &task, "One").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let note = key(&items[1])?;
    let closed = server.read_item(&ada, HEAP, &task).await?;
    server.set_state(&ada, HEAP, &closed, "done").await?;

    // Act
    let to_note = cli.run(&["step", "add", note, "Nope"]).await;
    let to_closed = cli.run(&["step", "add", &task, "Nope"]).await;
    let tick_closed = cli.run(&["step", "tick", &format!("{task}.1")]).await;

    // Assert
    expect_that!(to_note.exited(3), eq(true));
    expect_that!(to_note.stderr, contains_substring("note"));
    expect_that!(to_closed.exited(3), eq(true));
    expect_that!(to_closed.stderr, contains_substring("hint: reopen it first"));
    expect_that!(tick_closed.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_lists_the_steps_with_their_marks_and_contexts() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{task}/steps"),
            Some(json!({ "title": "Inventory the certs", "context": "All of them\n\tincluding the old ones" })),
        )
        .await?;
    add_step(&server, &ada, &task, "Run in staging").await?;
    add_step(&server, &ada, &task, "Email ops").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["step", "tick", &format!("{task}.1")]).await;
    cli.run(&["step", "drop", &format!("{task}.3")]).await;

    // Act
    let shown = cli.run(&["item", "show", &task]).await;
    let json = cli.run(&["item", "show", &task, "--json"]).await;

    // Assert
    expect_that!(
        shown.stdout,
        ends_with(format!(
            "\nSteps 1/2\n\
             [x] {task}.1  Inventory the certs\n\
             \x20       All of them\n\
             \x20           including the old ones\n\
             [ ] {task}.2  Run in staging\n\
             [-] {task}.3  Email ops  dropped\n"
        ))
    );
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["step_progress"], eq(&json!({ "ticked": 1, "total": 2 })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_has_no_steps_section_for_an_item_without_steps() -> Result<()> {
    // Arrange
    let (server, ada, items, _) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let shown = cli.run(&["item", "show", key(&items[1])?]).await;

    // Assert
    expect_that!(shown.stdout, not(contains_substring("Steps")));
    Ok(())
}
