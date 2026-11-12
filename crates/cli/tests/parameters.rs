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

const PARAMETERS: &str = "/teams/ada/heaps/personal/parameters";

/// The parameter named `name` among those in use, as the server lists it.
async fn listed(server: &TestServer, ada: &common::TestUser, name: &str) -> Result<Value> {
    let list = server.send(ada, "GET", PARAMETERS, None).await?;
    list["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|parameter| parameter["name"] == name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no parameter {name} in use"))
}

fn labels(parameter: &Value) -> Vec<String> {
    parameter["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| option["label"].as_str().map(str::to_owned))
        .collect()
}

#[gtest]
#[tokio::test]
async fn parameter_add_creates_it_with_its_weight_options_and_tags() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let infra = server.send(&ada, "GET", TAGS, None).await?["tags"][0]["id"].clone();

    // Act
    let added = cli
        .run(&[
            "parameter",
            "add",
            "Area",
            "--weight",
            "30",
            "--option",
            "Backend",
            "--option",
            "Frontend",
            "--tag",
            "Infra",
        ])
        .await;
    let quiet = cli.run(&["parameter", "add", "Team", "-q"]).await;
    let json = cli.run(&["parameter", "add", "Cost", "--json"]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stdout, eq("Added parameter \"Area\" with 2 options.\n"));
    let area = listed(&server, &ada, "Area").await?;
    expect_that!(area["weight"], eq(30));
    expect_that!(labels(&area), elements_are![eq("Backend"), eq("Frontend")]);
    expect_that!(area["tag_ids"], eq(&json!([infra])));
    expect_that!(quiet.stdout, eq("Team\n"));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["name"], eq("Cost"));
    expect_that!(body["weight"], eq(10));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_add_refuses_a_weight_out_of_range_and_an_unknown_tag_and_adds_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let heavy = cli.run(&["parameter", "add", "Area", "--weight", "101"]).await;
    let untagged = cli.run(&["parameter", "add", "Area", "--tag", "Nope"]).await;

    // Assert
    expect_that!(heavy.exited(2), eq(true));
    expect_that!(untagged.exited(3), eq(true));
    let list = server.send(&ada, "GET", PARAMETERS, None).await?;
    expect_that!(list["parameters"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_edit_renames_and_reweighs() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;

    // Act
    let edited = cli
        .run(&["parameter", "edit", "area", "--name", "Team", "--weight", "40"])
        .await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq("Renamed parameter \"Area\" to \"Team\".\n"));
    let team = listed(&server, &ada, "Team").await?;
    expect_that!(team["weight"], eq(40));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_edit_tags_and_untags_against_the_current_links() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let infra = server.send(&ada, "GET", TAGS, None).await?["tags"][0]["id"].clone();
    let ops = server.send(&ada, "POST", TAGS, Some(json!({ "name": "Ops" }))).await?["id"].clone();
    server.add_parameter(&ada, "Area", &["Backend"], &[&infra]).await?;

    // Act
    let edited = cli
        .run(&["parameter", "edit", "Area", "--tag", "Ops", "--untag", "Infra"])
        .await;
    let again = cli.run(&["parameter", "edit", "Area", "--tag", "Ops"]).await;
    let both = cli
        .run(&["parameter", "edit", "Area", "--tag", "Ops", "--untag", "ops"])
        .await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq("Updated parameter \"Area\".\n"));
    expect_that!(listed(&server, &ada, "Area").await?["tag_ids"], eq(&json!([ops])));
    expect_that!(again.stdout, eq("No change to parameter \"Area\".\n"));
    expect_that!(both.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_edit_order_rearranges_the_options_and_checks_the_whole_order_first() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;

    // Act
    let missing = cli.run(&["parameter", "edit", "Area", "--order", "Frontend"]).await;
    let repeated = cli
        .run(&["parameter", "edit", "Area", "--order", "Backend,Backend,Frontend"])
        .await;
    let unchanged = cli
        .run(&["parameter", "edit", "Area", "--order", "Backend, Frontend"])
        .await;
    let swapped = cli
        .run(&["parameter", "edit", "Area", "--order", "Frontend", "--order", "Backend"])
        .await;

    // Assert
    expect_that!(missing.exited(2), eq(true));
    expect_that!(
        missing.stderr,
        contains_substring("--order must name every option of Area once: missing Backend")
    );
    expect_that!(repeated.exited(2), eq(true));
    expect_that!(repeated.stderr, contains_substring("--order names Backend twice"));
    expect_that!(unchanged.stdout, eq("No change to parameter \"Area\".\n"));
    expect_that!(swapped.exited(0), eq(true));
    expect_that!(swapped.stdout, eq("Updated parameter \"Area\".\n"));
    expect_that!(
        labels(&listed(&server, &ada, "Area").await?),
        elements_are![eq("Frontend"), eq("Backend")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_edit_applies_name_tags_and_order_in_one_call() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let infra = server.send(&ada, "GET", TAGS, None).await?["tags"][0]["id"].clone();
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;

    // Act
    let edited = cli
        .run(&[
            "parameter",
            "edit",
            "Area",
            "--name",
            "Team",
            "--tag",
            "Infra",
            "--order",
            "Frontend,Backend",
        ])
        .await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(edited.stdout, eq("Renamed parameter \"Area\" to \"Team\".\n"));
    let team = listed(&server, &ada, "Team").await?;
    expect_that!(team["tag_ids"], eq(&json!([infra])));
    expect_that!(labels(&team), elements_are![eq("Frontend"), eq("Backend")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_edit_with_nothing_to_change_sends_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let area = server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;

    // Act
    let edited = cli.run(&["parameter", "edit", "Area"]).await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(
        edited.stdout,
        eq("Nothing to change: give --name, --weight, --tag, --untag or --order.\n")
    );
    expect_that!(listed(&server, &ada, "Area").await?["version"], eq(&area["version"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_archive_archives_once_and_says_so_the_second_time() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;

    // Act
    let archived = cli.run(&["parameter", "archive", "Area"]).await;
    let again = cli.run(&["parameter", "archive", "area"]).await;
    let again_json = cli.run(&["parameter", "archive", "Area", "--json"]).await;
    let unknown = cli.run(&["parameter", "archive", "Nope"]).await;

    // Assert
    expect_that!(archived.exited(0), eq(true));
    expect_that!(archived.stdout, eq("Archived parameter \"Area\".\n"));
    let list = server.send(&ada, "GET", PARAMETERS, None).await?;
    expect_that!(list["parameters"], eq(&json!([])));
    expect_that!(list["archived"][0]["name"], eq("Area"));
    expect_that!(again.exited(0), eq(true));
    expect_that!(again.stdout, eq("Parameter \"Area\" is archived already.\n"));
    let body: Value = serde_json::from_str(&again_json.stdout)?;
    expect_that!(body["name"], eq("Area"));
    expect_that!(unknown.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_archive_is_refused_while_an_open_item_has_a_value_for_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let area = server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;
    let key = common::key(&items[0])?;
    server
        .edit_item(
            &ada,
            common::HEAP,
            key,
            json!({ "parameter_values": { area["id"].as_str().unwrap_or_default(): area["options"][0]["id"] } }),
        )
        .await?;

    // Act
    let archived = cli.run(&["parameter", "archive", "Area"]).await;

    // Assert
    expect_that!(archived.exited(3), eq(true));
    expect_that!(archived.stderr, contains_substring("parameter_in_use"));
    listed(&server, &ada, "Area").await?;
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_option_add_puts_the_option_last() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;

    // Act
    let added = cli.run(&["parameter", "option", "add", "Area", "Ops"]).await;
    let quiet = cli.run(&["parameter", "option", "add", "Area", "Docs", "-q"]).await;

    // Assert
    expect_that!(added.exited(0), eq(true));
    expect_that!(added.stdout, eq("Added option \"Ops\" to \"Area\".\n"));
    expect_that!(quiet.stdout, eq("Docs\n"));
    expect_that!(
        labels(&listed(&server, &ada, "Area").await?),
        elements_are![eq("Backend"), eq("Frontend"), eq("Ops"), eq("Docs")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_option_edit_relabels_and_keeps_the_order() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;

    // Act
    let edited = cli
        .run(&["parameter", "option", "edit", "Area", "Backend", "--label", "Server"])
        .await;
    let same = cli
        .run(&["parameter", "option", "edit", "Area", "Server", "--label", "Server"])
        .await;
    let bare = cli.run(&["parameter", "option", "edit", "Area", "Server"]).await;

    // Assert
    expect_that!(edited.exited(0), eq(true));
    expect_that!(
        edited.stdout,
        eq("Renamed option \"Backend\" to \"Server\" in \"Area\".\n")
    );
    expect_that!(
        labels(&listed(&server, &ada, "Area").await?),
        elements_are![eq("Server"), eq("Frontend")]
    );
    expect_that!(same.stdout, eq("No change to option \"Server\" in \"Area\".\n"));
    expect_that!(bare.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_option_archive_moves_open_items_to_the_replacement() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let area = server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;
    let key = common::key(&items[0])?;
    server
        .edit_item(
            &ada,
            common::HEAP,
            key,
            json!({ "parameter_values": { area["id"].as_str().unwrap_or_default(): area["options"][0]["id"] } }),
        )
        .await?;

    // Act
    let itself = cli
        .run(&[
            "parameter",
            "option",
            "archive",
            "Area",
            "Backend",
            "--replace-with",
            "backend",
        ])
        .await;
    let archived = cli
        .run(&[
            "parameter",
            "option",
            "archive",
            "Area",
            "Backend",
            "--replace-with",
            "Frontend",
        ])
        .await;

    // Assert
    expect_that!(itself.exited(2), eq(true));
    expect_that!(archived.exited(0), eq(true));
    expect_that!(
        archived.stdout,
        eq("Archived option \"Backend\" in \"Area\"; open items with it now have \"Frontend\".\n")
    );
    expect_that!(
        labels(&listed(&server, &ada, "Area").await?),
        elements_are![eq("Frontend")]
    );
    let item = server.read_item(&ada, common::HEAP, key).await?;
    expect_that!(item["parameters"][0]["option"]["label"], eq("Frontend"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_option_archive_again_says_it_is_archived_already_and_changes_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server
        .add_parameter(&ada, "Area", &["Backend", "Frontend"], &[])
        .await?;
    let args = [
        "parameter",
        "option",
        "archive",
        "Area",
        "Backend",
        "--replace-with",
        "Frontend",
    ];
    cli.run(&args).await;
    let version = listed(&server, &ada, "Area").await?["version"].clone();

    // Act
    let again = cli.run(&args).await;
    let json = cli.run(&[args.as_slice(), &["--json"]].concat()).await;

    // Assert
    expect_that!(again.exited(0), eq(true));
    expect_that!(
        again.stdout,
        eq("Option \"Backend\" in \"Area\" is archived already.\n")
    );
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["archived_options"][0]["label"], eq("Backend"));
    expect_that!(listed(&server, &ada, "Area").await?["version"], eq(&version));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_edit_that_fails_after_a_write_says_what_was_saved() -> Result<()> {
    // Arrange
    // The tags write is the one the server can refuse that the client can't check first: a parameter takes at most
    // 50 tags, and the 51 here are all the heap's own, so each one resolves.
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;
    let mut names = vec!["Infra".to_owned()];
    for n in 0..50 {
        let name = format!("Tag {n}");
        server.send(&ada, "POST", TAGS, Some(json!({ "name": name }))).await?;
        names.push(name);
    }
    let mut args = vec!["parameter", "edit", "Area", "--name", "Team"];
    for name in &names {
        args.extend(["--tag", name]);
    }
    let mut json_args = vec!["parameter", "edit", "Team", "--name", "Squad"];
    for name in &names {
        json_args.extend(["--tag", name]);
    }
    json_args.push("--json");

    // Act
    let human = cli.run(&args).await;
    let json = cli.run(&json_args).await;

    // Assert
    expect_that!(human.exited(3), eq(true));
    expect_that!(human.stdout, eq(""));
    expect_that!(
        human.stderr,
        contains_substring(
            "Parameter \"Team\" got its new name, but not its tags; run the edit again with --tag/--untag."
        )
    );
    // The rename was saved, tags unchanged. With --json stdout has the renamed parameter and stderr only the problem.
    expect_that!(listed(&server, &ada, "Squad").await?["tag_ids"], eq(&json!([])));
    expect_that!(json.exited(3), eq(true));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["name"], eq("Squad"));
    expect_that!(json.stderr, contains_substring("\"status\""));
    expect_that!(json.stderr, not(contains_substring("got its new")));
    Ok(())
}
