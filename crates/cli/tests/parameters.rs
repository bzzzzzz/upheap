//! `upheap parameter list`: what it prints for a heap's parameters, options and tags, and what it leaves out.

mod common;

use anyhow::Result;
use common::{TestServer, ada_with_items, cli_in_heap};
use googletest::prelude::*;
use serde_json::{Value, json};

const TAGS: &str = "/teams/ada/heaps/personal/tags";

#[gtest]
#[tokio::test]
async fn parameter_list_prints_each_parameter_with_its_options_and_the_tags_it_is_for() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let tags = server.send(&ada, "GET", TAGS, None).await?;
    let infra = tags["tags"][0]["id"].clone();
    let ops = server.send(&ada, "POST", TAGS, Some(json!({ "name": "Ops" }))).await?["id"].clone();
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[&infra, &ops])
        .await?;
    server.add_parameter(&ada, "Customer", &[], &[]).await?;

    // Act
    let human = cli.run(&["parameter", "list"]).await;
    let quiet = cli.run(&["parameter", "list", "-q"]).await;
    let json = cli.run(&["parameter", "list", "--json"]).await;

    // Assert
    expect_that!(human.exited(0), eq(true));
    expect_that!(
        human.stdout,
        eq("Area      Backend, Frontend  for Infra, Ops\nCustomer  no options\n")
    );
    expect_that!(human.stderr, eq(""));
    expect_that!(quiet.stdout, eq("Area\nCustomer\n"));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["parameters"][0]["name"], eq("Area"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_list_in_a_heap_without_parameters_says_so() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let listed = cli.run(&["parameter", "list"]).await;
    let quiet = cli.run(&["parameter", "list", "-q"]).await;

    expect_that!(listed.exited(0), eq(true));
    expect_that!(listed.stdout, eq("No parameters in ada/personal yet.\n"));
    expect_that!(quiet.stdout, eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_list_leaves_out_an_archived_parameter_and_says_so_on_stderr() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;
    let old = server.add_parameter(&ada, "Old", &["X"], &[]).await?;
    let path = format!(
        "/teams/ada/heaps/personal/parameters/{}/archive",
        old["id"].as_str().unwrap_or_default()
    );
    server.send(&ada, "POST", &path, None).await?;

    // Act
    let listed = cli.run(&["parameter", "list"]).await;
    let quiet = cli.run(&["parameter", "list", "-q"]).await;

    // Assert
    expect_that!(listed.exited(0), eq(true));
    expect_that!(listed.stdout, eq("Area  Backend\n"));
    expect_that!(
        listed.stderr,
        eq("1 archived parameter isn't shown; --json lists it.\n")
    );
    expect_that!(quiet.stdout, eq("Area\n"));
    expect_that!(quiet.stderr, eq(""));
    Ok(())
}
