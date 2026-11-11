//! Reminders in the command line: `item add --reminder`, the group in `item list`, `item show`, `item to-task`, and
//! a reminder's date.

mod common;

use anyhow::{Context, Result};
use common::{HEAP, TestServer, ada_with_items, cli_in_heap};
use googletest::prelude::*;
use serde_json::{Value, json};

#[gtest]
#[tokio::test]
async fn adding_a_reminder_says_when_it_joins_the_list() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli
        .run(&["item", "add", "Renew the domain", "--reminder", "2099-03-12"])
        .await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stderr, eq(""));
    expect_that!(added.stdout, starts_with("Added reminder \"Renew the domain\" ("));
    expect_that!(added.stdout, ends_with("); it joins the list on 2099-03-12.\n"));
    let key = added
        .stdout
        .split_once(" (")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(key, _)| key)
        .context("a key")?;
    let item = server.read_item(&ada, HEAP, key).await?;
    expect_that!(item["kind"], eq("reminder"));
    expect_that!(item["due"]["date"], eq("2099-03-12"));
    expect_that!(item["due"]["firm"], eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_for_today_or_the_past_lands_in_the_list_like_any_item() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let added = cli.run(&["item", "add", "Overdue", "--reminder", "2020-01-02"]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stdout, starts_with("Added reminder \"Overdue\" ("));
    expect_that!(added.stdout, contains_substring("), now "));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn quiet_prints_the_key_and_json_the_body() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let quiet = cli.run(&["item", "add", "A", "--reminder", "2099-03-12", "-q"]).await;
    let json = cli
        .run(&["item", "add", "B", "--reminder", "2099-03-12", "--json"])
        .await;

    // Assert
    expect_that!(quiet.stdout.trim().len(), eq(5));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["kind"], eq("reminder"));
    expect_that!(body["arrives_at"].is_i64(), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_doesnt_go_with_due_or_task() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let with_due = cli
        .run(&["item", "add", "X", "--reminder", "2099-03-12", "--due", "2099-03-13"])
        .await;
    let with_task = cli
        .run(&["item", "add", "X", "--reminder", "2099-03-12", "--task"])
        .await;
    let on_all_heaps = cli.run(&["item", "list", "--all-heaps", "--reminders", "shown"]).await;
    let on_closed = cli.run(&["item", "list", "--closed", "--reminders", "shown"]).await;

    // Assert
    for refused in [&with_due, &with_task, &on_all_heaps, &on_closed] {
        expect_that!(refused.exited(2), eq(true));
        expect_that!(refused.stdout, eq(""));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_list_hides_reminders_to_come_until_asked_and_search_finds_them() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["item", "add", "Renew the domain", "--reminder", "2099-03-12"])
        .await;

    // Act
    let hidden = cli.run(&["item", "list"]).await;
    let shown = cli.run(&["item", "list", "--reminders", "shown"]).await;
    let found = cli.run(&["item", "list", "domain"]).await;
    let missed = cli
        .run(&["item", "list", "--reminders", "shown", "nothing like it"])
        .await;
    let quiet = cli.run(&["item", "list", "--reminders", "shown", "-q"]).await;
    let json = cli.run(&["item", "list", "--reminders", "shown", "--json"]).await;

    // Assert
    expect_that!(hidden.stdout, not(contains_substring("Renew the domain")));
    expect_that!(hidden.stdout, not(contains_substring("Reminders to come")));
    expect_that!(shown.stdout, contains_substring("Reminders to come\n"));
    let row = shown
        .stdout
        .lines()
        .skip_while(|line| *line != "Reminders to come")
        .nth(1)
        .context("a row under the heading")?;
    expect_that!(row, contains_substring("Renew the domain"));
    expect_that!(row, contains_substring("2099-03-12"));
    expect_that!(found.stdout, contains_substring("Renew the domain"));
    expect_that!(found.stdout, not(contains_substring("Rotate the staging certs")));
    expect_that!(missed.stdout, not(contains_substring("Renew the domain")));
    // The reminders come after the ranked keys.
    expect_that!(quiet.stdout.lines().count(), eq(4));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["reminders"][0]["title"], eq("Renew the domain"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_view_that_shows_reminders_is_overridden_by_the_flag() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["item", "add", "Renew the domain", "--reminder", "2099-03-12"])
        .await;
    server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/views",
            Some(json!({ "name": "Everything", "view": { "reminders": "shown" } })),
        )
        .await?;

    // Act
    let from_view = cli.run(&["item", "list", "--view", "Everything"]).await;
    let overridden = cli
        .run(&["item", "list", "--view", "Everything", "--reminders", "hidden"])
        .await;

    // Assert
    expect_that!(from_view.stdout, contains_substring("Renew the domain"));
    expect_that!(overridden.stdout, not(contains_substring("Renew the domain")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn show_says_a_reminder_to_come_joins_the_list_on_its_date() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let added = cli
        .run(&["item", "add", "Renew the domain", "--reminder", "2099-03-12", "-q"])
        .await;
    let key = added.stdout.trim();
    let past = cli
        .run(&["item", "add", "Overdue", "--reminder", "2020-01-02", "-q"])
        .await;

    // Act
    let shown = cli.run(&["item", "show", key]).await;
    let arrived = cli.run(&["item", "show", past.stdout.trim()]).await;

    // Assert
    expect_that!(shown.exited(0), eq(true));
    expect_that!(shown.stdout, contains_substring("Kind"));
    expect_that!(shown.stdout, contains_substring("reminder"));
    expect_that!(shown.stdout, contains_substring("Joins list"));
    expect_that!(shown.stdout, contains_substring("2099-03-12"));
    expect_that!(arrived.stdout, contains_substring("reminder"));
    expect_that!(arrived.stdout, not(contains_substring("Joins list")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reminder_becomes_a_task_and_its_date_moves_but_cant_be_cleared() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = cli
        .run(&["item", "add", "Renew the domain", "--reminder", "2099-03-12", "-q"])
        .await
        .stdout
        .trim()
        .to_owned();

    // Act
    let moved = cli.run(&["item", "edit", &key, "--due", "2099-04-01"]).await;
    let cleared = cli.run(&["item", "edit", &key, "--no-due"]).await;
    let made = cli.run(&["item", "to-task", &key]).await;

    // Assert
    expect_that!(moved.exited(0), eq(true));
    expect_that!(cleared.exited(3), eq(true));
    expect_that!(made.stdout, eq(&format!("Made {key} a task: Renew the domain.\n")));
    let item = server.read_item(&ada, HEAP, &key).await?;
    expect_that!(item["kind"], eq("task"));
    expect_that!(item["due"]["date"], eq("2099-04-01"));
    Ok(())
}
