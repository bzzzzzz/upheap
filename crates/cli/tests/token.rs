//! `UPHEAP_TOKEN`: a personal token every command uses instead of the keychain, and what a refused one says.

mod common;

use anyhow::{Context, Result};
use common::{HEAP, TestCli, TestServer, TestUser};
use googletest::prelude::*;
use serde_json::{Value, json};

const HINT: &str = "hint: make a new token on the web app's Account page, under Command line, and set UPHEAP_TOKEN to \
                    it; or unset UPHEAP_TOKEN to use the sign-in from upheap auth login\n";

/// A machine with no keychain sign-in and no config directory, whose environment holds `token`.
fn machine_with(server: &TestServer, token: &str) -> Result<TestCli> {
    let mut cli = TestCli::new(server)?;
    cli.env.token = Some(token.to_owned());
    cli.env.heap = Some(HEAP.to_owned());
    cli.env.config_dir = None;
    Ok(cli)
}

async fn make_token(server: &TestServer, user: &TestUser, expires_in: &str) -> Result<(String, String)> {
    let made = server
        .send(
            user,
            "POST",
            "/me/personal-tokens",
            Some(json!({ "name": "ci", "expires_in": expires_in })),
        )
        .await?;
    Ok((
        made["secret"].as_str().context("secret")?.to_owned(),
        made["token"]["id"].as_str().context("id")?.to_owned(),
    ))
}

#[gtest]
#[tokio::test]
async fn a_personal_token_adds_an_item_with_no_keychain_and_no_config_directory() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let (secret, _) = make_token(&server, &ada, "90_days").await?;
    let cli = machine_with(&server, &secret)?;

    // Act
    let added = cli.run(&["item", "add", "Rotate the certs"]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stderr, eq(""));
    let items = server
        .send(&ada, "GET", "/teams/ada/heaps/personal/items", None)
        .await?;
    expect_that!(items["ranked"][0]["item"]["title"], eq(&json!("Rotate the certs")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_revoked_token_exits_6_and_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let (secret, id) = make_token(&server, &ada, "90_days").await?;
    let cli = machine_with(&server, &secret)?;
    server
        .send(&ada, "DELETE", &format!("/me/personal-tokens/{id}"), None)
        .await?;

    // Act
    let added = cli.run(&["item", "add", "Rotate the certs"]).await;

    // Assert
    expect_that!(added.exited(6), eq(true));
    expect_that!(added.stdout, eq(""));
    expect_that!(
        added.stderr,
        eq(&format!(
            "error: the server refused the token in UPHEAP_TOKEN: it was revoked\n{HINT}"
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_token_revoked_by_a_password_change_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let (secret, _) = make_token(&server, &ada, "90_days").await?;
    let cli = machine_with(&server, &secret)?;
    server
        .send(
            &ada,
            "POST",
            "/me/password",
            Some(json!({ "current_password": "a long test password", "password": "another long test password" })),
        )
        .await?;

    // Act
    let added = cli.run(&["item", "add", "Rotate the certs"]).await;

    // Assert
    expect_that!(added.exited(6), eq(true));
    expect_that!(
        added.stderr,
        eq(&format!(
            "error: the server refused the token in UPHEAP_TOKEN: it was revoked because the account's password changed\n{HINT}"
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_expired_token_exits_6_and_says_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let (secret, _) = make_token(&server, &ada, "30_days").await?;
    let cli = machine_with(&server, &secret)?;
    server.advance(domain::Duration::from_days(31));

    // Act
    let listed = cli.run(&["item", "list"]).await;

    // Assert
    expect_that!(listed.exited(6), eq(true));
    expect_that!(
        listed.stderr,
        eq(&format!(
            "error: the server refused the token in UPHEAP_TOKEN: it has expired\n{HINT}"
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_token_the_server_does_not_know_exits_6_and_json_keeps_the_problem_body() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    server.register("ada", "Ada").await?;
    let cli = machine_with(&server, "not a token")?;

    // Act
    let listed = cli.run(&["item", "list"]).await;
    let json = cli.run(&["item", "list", "--json"]).await;

    // Assert
    expect_that!(listed.exited(6), eq(true));
    expect_that!(
        listed.stderr,
        eq(&format!(
            "error: the server refused the token in UPHEAP_TOKEN: it isn't a token this server knows\n{HINT}"
        ))
    );
    expect_that!(json.exited(6), eq(true));
    let problem: Value = serde_json::from_str(&json.stderr)?;
    expect_that!(problem["code"], eq(&json!("unauthenticated")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_hint_is_the_same_in_a_run() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    server.register("ada", "Ada").await?;
    let mut cli = machine_with(&server, "uph_unknown")?;
    cli.env.run = Some("7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40".to_owned());

    // Act
    let listed = cli.run(&["item", "list"]).await;

    // Assert
    expect_that!(listed.exited(6), eq(true));
    expect_that!(listed.stderr, ends_with(HINT));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn with_a_token_and_no_server_the_setup_error_comes_first() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let mut cli = machine_with(&server, "uph_unknown")?;
    cli.env.server = None;

    // Act
    let listed = cli.run(&["item", "list"]).await;

    // Assert
    expect_that!(listed.exited(2), eq(true));
    expect_that!(listed.stderr, contains_substring("or set UPHEAP_SERVER"));
    Ok(())
}
