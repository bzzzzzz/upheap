//! `upheap item start | block | done | wontdo | reopen`: each verb's state, repeating one, and the server's refusals.

mod common;

use anyhow::Result;
use cli::TokenStore;
use common::{HEAP, TestCli, TestServer, ada_with_items, cli_in_heap, key};
use domain::Duration;
use googletest::prelude::*;
use serde_json::Value;

#[gtest]
#[tokio::test]
async fn each_verb_moves_the_item_and_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    let title = "Rotate the staging certs";

    // Act and assert, one verb after another
    for (verb, line, state) in [
        ("start", format!("Started {key}: {title}.\n"), "in_progress"),
        ("done", format!("Done {key}: {title}.\n"), "done"),
        ("reopen", format!("Reopened {key}: {title}.\n"), "open"),
        ("wontdo", format!("Won't do {key}: {title}.\n"), "wont_do"),
        ("reopen", format!("Reopened {key}: {title}.\n"), "open"),
        ("block", format!("Blocked {key}: {title}.\n"), "blocked"),
    ] {
        let moved = cli.run(&["item", verb, key]).await;

        expect_that!(moved.stdout, eq(&line), "{verb}");
        expect_that!(moved.stderr, eq(""), "{verb}");
        expect_that!(moved.exited(0), eq(true), "{verb}");
        expect_that!(server.read_item(&ada, HEAP, key).await?["state"], eq(state), "{verb}");
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn repeating_a_verb_writes_nothing_and_says_it_is_already_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.run(&["item", "done", key]).await;
    let version = server.read_item(&ada, HEAP, key).await?["version"].clone();

    // Act
    let again = cli.run(&["item", "done", key]).await;
    let as_json = cli.run(&["item", "done", key, "--json"]).await;
    let quiet = cli.run(&["item", "done", key, "-q"]).await;

    // Assert
    expect_that!(again.stdout, eq(&format!("{key} is already done.\n")));
    expect_that!(again.exited(0), eq(true));
    let read: Value = serde_json::from_str(&as_json.stdout)?;
    expect_that!(read["state"], eq("done"));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(server.read_item(&ada, HEAP, key).await?["version"], eq(&version));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_state_change_is_quiet_with_quiet_and_the_item_with_json() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let quiet = cli.run(&["item", "start", key(&items[0])?, "--quiet"]).await;
    let json = cli.run(&["item", "start", key(&items[1])?, "--json"]).await;

    expect_that!((quiet.stdout.as_str(), quiet.exited(0)), eq(("", true)));
    let moved: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(moved["state"], eq("in_progress"));
    expect_that!(moved["key"], eq(key(&items[1])?));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn closing_a_wont_do_item_as_done_says_to_reopen_it_first() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.run(&["item", "wontdo", key]).await;

    // Act
    let done = cli.run(&["item", "done", key]).await;
    let as_json = cli.run(&["item", "done", key, "--json"]).await;

    // Assert
    expect_that!(done.exited(3), eq(true));
    expect_that!(done.stdout, eq(""));
    expect_that!(done.stderr, contains_substring("(item_closed)"));
    expect_that!(
        done.stderr,
        ends_with(format!("hint: reopen it first with `upheap item reopen {key}`\n"))
    );
    let problem: Value = serde_json::from_str(&as_json.stderr)?;
    expect_that!(problem["code"], eq("item_closed"));
    expect_that!(server.read_item(&ada, HEAP, key).await?["state"], eq("wont_do"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn blocking_names_the_blocker_and_changes_it_on_request() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let (a, b, c) = (key(&items[0])?, key(&items[1])?, key(&items[2])?);

    // Act
    let blocked = cli.run(&["item", "block", a, "--by", b]).await;
    let blocked_by_b = server.read_item(&ada, HEAP, a).await?;
    let again = cli.run(&["item", "block", a]).await;
    let same_blocker = cli.run(&["item", "block", a, "--by", b]).await;
    let other = cli
        .run(&["item", "block", a, "--by", &format!("ada/personal/{c}")])
        .await;
    let blocked_by_c = server.read_item(&ada, HEAP, a).await?;

    // Assert
    expect_that!(
        blocked.stdout,
        eq(&format!("Blocked {a}: Rotate the staging certs. Waiting for {b}.\n"))
    );
    expect_that!(blocked_by_b["state"], eq("blocked"));
    expect_that!(blocked_by_b["blocked_by"]["key"], eq(b));
    expect_that!(again.stdout, eq(&format!("{a} is already blocked.\n")));
    expect_that!(same_blocker.stdout, eq(&format!("{a} is already blocked.\n")));
    expect_that!(other.exited(0), eq(true));
    expect_that!(blocked_by_c["blocked_by"]["key"], eq(c));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bare_blocker_key_is_in_the_heap_of_the_item_named_by_path() -> Result<()> {
    // Arrange: no default heap, so only the item's own path says where anything is
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let (a, b) = (key(&items[0])?, key(&items[1])?);

    // Act
    let blocked = cli.run(&["item", "block", &format!("{HEAP}/{a}"), "--by", b]).await;

    // Assert
    expect_that!(blocked.stderr, eq(""));
    expect_that!(blocked.exited(0), eq(true));
    expect_that!(server.read_item(&ada, HEAP, a).await?["blocked_by"]["key"], eq(b));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_cannot_wait_for_itself() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let a = key(&items[0])?;

    let blocked = cli.run(&["item", "block", a, "--by", a]).await;

    expect_that!(blocked.exited(3), eq(true));
    expect_that!(blocked.stderr, contains_substring("(blocking_cycle)"));
    expect_that!(server.read_item(&ada, HEAP, a).await?["state"], eq("open"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_blocker_the_server_lacks_is_not_found() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let blocked = cli.run(&["item", "block", key(&items[0])?, "--by", "zzzzz"]).await;

    expect_that!(blocked.exited(4), eq(true));
    expect_that!(blocked.stderr, starts_with("error: no item `zzzzz` in ada/personal"));
    expect_that!(blocked.stderr, contains_substring("`upheap item list`"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn reopening_after_the_reopen_window_is_refused() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let key = key(&items[0])?;
    cli.run(&["item", "done", key]).await;
    server.advance(Duration::from_days(8));
    // The tokens signed in before the clock moved may have run out on the server's clock; sign in again.
    cli.tokens.save(&server.url, &server.sign_in_cli(&ada).await?)?;

    // Act
    let reopened = cli.run(&["item", "reopen", key]).await;

    // Assert
    expect_that!(reopened.exited(3), eq(true));
    expect_that!(reopened.stderr, contains_substring("(reopen_window_expired)"));
    expect_that!(server.read_item(&ada, HEAP, key).await?["state"], eq("done"));
    Ok(())
}
