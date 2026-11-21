//! Team parameters through the CLI: making one for a team, a heap using it with its own weight, and the ways the
//! heap's own commands leave it to the team.

mod common;

use anyhow::Result;
use common::{HEAP, TestServer, ada_with_items, cli_in_heap};
use googletest::prelude::*;
use serde_json::{Value, json};

#[gtest]
#[tokio::test]
async fn the_team_parameters_path_from_making_one_to_stopping_its_use() -> Result<()> {
    // Arrange: Ada's personal team, with a heap and items.
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act and assert, step 1: make a team parameter; the team's list has it.
    let made = cli
        .run(&[
            "parameter",
            "add",
            "Tier",
            "--team",
            "ada",
            "--option",
            "Gold",
            "--option",
            "Silver",
        ])
        .await;
    expect_that!(made.exited(0), eq(true));
    expect_that!(
        made.stdout,
        eq("Added team parameter \"Tier\" to ada with 2 options.\n")
    );
    let team_list = cli.run(&["parameter", "list", "--team", "ada"]).await;
    expect_that!(team_list.stdout, eq("Tier  Gold, Silver\n"));

    // Step 2: the heap doesn't use it yet, and its list says it could.
    let before = cli.run(&["parameter", "list"]).await;
    expect_that!(before.stdout, eq("No parameters in ada/personal yet.\n"));
    expect_that!(
        before.stderr,
        eq("Team parameters this heap could use: Tier; start with `upheap parameter use <name>`.\n")
    );

    // Step 3: use it; the heap lists it marked team, with the first weight.
    let used = cli.run(&["parameter", "use", "tier"]).await;
    expect_that!(used.exited(0), eq(true));
    expect_that!(
        used.stdout,
        eq(
            "ada/personal uses team parameter \"Tier\" with weight 10; change the weight with `upheap heap weights --parameter \"Tier=N\"`.\n"
        )
    );
    let in_heap = cli.run(&["parameter", "list"]).await;
    expect_that!(in_heap.stdout, eq("Tier  Gold, Silver  team\n"));

    // Step 4: the heap's weight is set with the heap's weights, and only in this heap.
    let weighed = cli.run(&["heap", "weights", "--parameter", "Tier=40"]).await;
    expect_that!(weighed.exited(0), eq(true));
    let json = cli.run(&["parameter", "list", "--json"]).await;
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["parameters"][0]["weight"], eq(40));
    expect_that!(body["parameters"][0]["kind"], eq("team"));

    // Step 5: the heap's own commands send the person to the team.
    let renamed = cli.run(&["parameter", "edit", "Tier", "--name", "Level"]).await;
    expect_that!(renamed.exited(2), eq(true));
    expect_that!(renamed.stderr, contains_substring("is a parameter of the team ada"));
    let unweighed = cli.run(&["parameter", "edit", "Tier", "--weight", "5"]).await;
    expect_that!(unweighed.exited(2), eq(true));

    // Step 6: a rename on the team shows in the heap.
    let renamed = cli
        .run(&["parameter", "edit", "Tier", "--team", "ada", "--name", "Level"])
        .await;
    expect_that!(renamed.exited(0), eq(true));
    expect_that!(
        renamed.stdout,
        eq("Renamed team parameter \"Tier\" to \"Level\" in every heap of ada.\n")
    );
    let ordered = cli
        .run(&["parameter", "edit", "Level", "--team", "ada", "--order", "Silver,Gold"])
        .await;
    expect_that!(ordered.exited(0), eq(true));
    let added = cli
        .run(&["parameter", "option", "add", "Level", "Bronze", "--team", "ada"])
        .await;
    expect_that!(added.exited(0), eq(true));
    let in_heap = cli.run(&["parameter", "list"]).await;
    expect_that!(in_heap.stdout, eq("Level  Silver, Gold, Bronze  team\n"));

    // Step 7: an item takes a value, so the heap can't stop using it until the item is done with.
    let item = cli
        .run(&["item", "add", "Call the customer", "--parameter", "Level=Gold", "-q"])
        .await;
    expect_that!(item.exited(0), eq(true));
    let refused = cli.run(&["parameter", "stop-using", "Level"]).await;
    expect_that!(refused.exited(3), eq(true));
    expect_that!(refused.stderr, contains_substring("parameter_in_use"));

    // Step 8: an option archived on the team moves the item, then the parameter is archived once no item has a
    // value.
    let archived = cli
        .run(&[
            "parameter",
            "option",
            "archive",
            "Level",
            "Gold",
            "--replace-with",
            "Bronze",
            "--team",
            "ada",
        ])
        .await;
    expect_that!(archived.exited(0), eq(true));
    let key = item.stdout.trim();
    let shown = cli.run(&["item", "show", key]).await;
    expect_that!(shown.stdout, contains_substring("Bronze"));
    let done = cli.run(&["item", "done", key]).await;
    expect_that!(done.exited(0), eq(true));
    let stopped = cli.run(&["parameter", "stop-using", "Level"]).await;
    expect_that!(stopped.exited(0), eq(true));
    expect_that!(
        stopped.stdout,
        eq("ada/personal no longer uses team parameter \"Level\".\n")
    );
    let again = cli.run(&["parameter", "list", "--heap", HEAP]).await;
    expect_that!(again.stdout, eq("No parameters in ada/personal yet.\n"));

    // Step 9: using it again brings back the weight it had.
    let back = cli.run(&["parameter", "use", "Level"]).await;
    expect_that!(back.stdout, contains_substring("with weight 40"));

    // Step 10: the team archives it once nothing needs it.
    let gone = cli.run(&["parameter", "archive", "Level", "--team", "ada"]).await;
    expect_that!(gone.exited(0), eq(true));
    let team_list = cli.run(&["parameter", "list", "--team", "ada"]).await;
    expect_that!(team_list.stdout, contains_substring("No team parameters in ada yet"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn flags_for_a_heap_parameter_are_refused_with_team_before_any_request() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let weighed = cli
        .run(&["parameter", "add", "Tier", "--team", "ada", "--weight", "30"])
        .await;
    let tagged = cli
        .run(&["parameter", "add", "Tier", "--team", "ada", "--tag", "Infra"])
        .await;
    let edited = cli
        .run(&["parameter", "edit", "Tier", "--team", "ada", "--weight", "30"])
        .await;

    expect_that!(weighed.exited(2), eq(true));
    expect_that!(tagged.exited(2), eq(true));
    expect_that!(edited.exited(2), eq(true));
    let list = server.send(&ada, "GET", "/teams/ada/parameters", None).await?;
    expect_that!(list["parameters"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_parameter_is_linked_to_team_tags_and_a_heap_links_its_own_tags() -> Result<()> {
    // Arrange: a team parameter in use in the heap, a team tag and a heap tag.
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    for command in [
        &["parameter", "add", "Tier", "--team", "ada"][..],
        &["parameter", "use", "Tier"],
        &["tag", "add", "billing", "--team", "ada"],
        &["tag", "add", "support"],
    ] {
        let done = cli.run(command).await;
        assert!(done.exited(0), "{command:?}: {}", done.stderr);
    }
    let parameter = async |cli: &common::TestCli| -> Result<Value> {
        let listed: Value = serde_json::from_str(&cli.run(&["parameter", "list", "--json"]).await.stdout)?;
        Ok(listed["parameters"][0].clone())
    };

    // Act
    let team_linked = cli
        .run(&["parameter", "edit", "Tier", "--team", "ada", "--tag", "billing"])
        .await;
    let heap_linked = cli.run(&["parameter", "edit", "Tier", "--tag", "support"]).await;
    let repeated = cli.run(&["parameter", "edit", "Tier", "--tag", "support"]).await;
    let both = parameter(&cli).await?;
    let heap_list = cli.run(&["parameter", "list"]).await;
    let team_list = cli.run(&["parameter", "list", "--team", "ada"]).await;
    let swapped = cli
        .run(&["parameter", "edit", "Tier", "--tag", "billing", "--untag", "support"])
        .await;
    let swapped_again = cli
        .run(&["parameter", "edit", "Tier", "--tag", "billing", "--untag", "support"])
        .await;
    let heap_relinked = cli.run(&["parameter", "edit", "Tier", "--tag", "support"]).await;
    let heap_unlinked = cli
        .run(&["parameter", "edit", "Tier", "--untag", "billing", "--untag", "support"])
        .await;
    let team_unlinked = cli
        .run(&["parameter", "edit", "Tier", "--team", "ada", "--untag", "billing"])
        .await;
    let none = parameter(&cli).await?;
    let renamed = cli
        .run(&["parameter", "edit", "Tier", "--tag", "support", "--name", "Level"])
        .await;

    // Assert
    expect_that!(
        team_linked.stdout,
        eq("Updated team parameter \"Tier\" in every heap of ada.\n")
    );
    expect_that!(
        heap_linked.stdout,
        eq("Updated the tags of team parameter \"Tier\" in this heap.\n")
    );
    expect_that!(repeated.stdout, eq("No change to team parameter \"Tier\".\n"));
    expect_that!(both["tag_ids"].as_array().map(Vec::len), some(eq(2)));
    expect_that!(both["team_tag_ids"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(both["heap_tag_ids"].as_array().map(Vec::len), some(eq(1)));
    expect_that!(heap_list.stdout, eq("Tier  no options  team, for billing, support\n"));
    expect_that!(team_list.stdout, eq("Tier  no options  for billing\n"));
    expect_that!(
        swapped.stdout,
        eq("Updated the tags of team parameter \"Tier\" in this heap.\n")
    );
    expect_that!(swapped_again.stdout, eq("No change to team parameter \"Tier\".\n"));
    expect_that!(heap_relinked.exited(0), eq(true));
    expect_that!(heap_unlinked.exited(0), eq(true));
    expect_that!(team_unlinked.exited(0), eq(true));
    expect_that!(none["tag_ids"], eq(&json!([])));
    expect_that!(renamed.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn promoting_a_heap_parameter_prints_the_reach_and_goes_ahead_when_confirmed() -> Result<()> {
    // Arrange: Tier in A (the default heap) and in B, linked to nothing, with an item in each.
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    server
        .send(&ada, "POST", "/teams/ada/heaps", Some(json!({ "name": "Beta" })))
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;
    for (heap, name, options) in [
        (HEAP, "Tier", ["--option", "Gold"]),
        ("ada/beta", "tier", ["--option", "Platinum"]),
    ] {
        let made = cli
            .run(&["parameter", "add", name, "--heap", heap, options[0], options[1]])
            .await;
        assert!(made.exited(0), "parameter add: {}", made.stderr);
    }
    let added = cli
        .run(&[
            "item",
            "add",
            "Chase",
            "--heap",
            "ada/beta",
            "--parameter",
            "tier=Platinum",
            "-q",
        ])
        .await;
    assert!(added.exited(0), "item add: {}", added.stderr);

    // Act
    let refused = cli.run(&["parameter", "promote", "Tier"]).await;
    let as_json = cli.run(&["parameter", "promote", "Tier", "--json"]).await;
    let (_, after) = refused
        .stderr
        .split_once("To go ahead, run:\n")
        .expect("the refusal names a command");
    let command: Vec<&str> = after
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .skip(1)
        .collect();
    let promoted = cli.run(&command).await;
    let again = cli.run(&command).await;
    let list = cli.run(&["parameter", "list", "--team", "ada"]).await;
    let in_beta = cli.run(&["parameter", "list", "--heap", "ada/beta"]).await;

    // Assert
    expect_that!(refused.exited(3), eq(true));
    expect_that!(
        refused.stderr,
        starts_with(
            "error: Making \"Tier\" a team parameter of ada merges these heap parameters into it (parameter_merge_unconfirmed):\n  \
             personal  Tier  0 items  (this heap)\n  \
             beta      tier  1 item  adds Platinum\n\
             Each heap keeps its weight. Rankings can shift in a heap whose options were in another order.\n\
             This can't be undone. To go ahead, run:\n"
        )
    );
    expect_that!(as_json.exited(3), eq(true));
    let body: Value = serde_json::from_str(&as_json.stderr)?;
    expect_that!(body["code"], eq("parameter_merge_unconfirmed"));
    expect_that!(
        body["parameter_merge_reach"]["entries"].as_array().map(Vec::len),
        some(eq(2))
    );
    expect_that!(promoted.exited(0), eq(true));
    expect_that!(promoted.stdout, eq("Made \"Tier\" a team parameter of ada.\n"));
    expect_that!(again.exited(0), eq(true));
    expect_that!(again.stdout, eq("\"Tier\" is already a team parameter of ada.\n"));
    expect_that!(list.stdout, eq("Tier  Gold, Platinum\n"));
    expect_that!(in_beta.stdout, eq("Tier  Gold, Platinum  team\n"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn promoting_a_parameter_linked_to_tags_is_refused_with_the_servers_message() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let made = cli.run(&["tag", "add", "billing"]).await;
    assert!(made.exited(0), "tag add: {}", made.stderr);
    let made = cli
        .run(&["parameter", "add", "Tier", "--option", "Gold", "--tag", "billing"])
        .await;
    assert!(made.exited(0), "parameter add: {}", made.stderr);

    // Act
    let refused = cli.run(&["parameter", "promote", "Tier"]).await;

    // Assert
    expect_that!(refused.exited(3), eq(true));
    expect_that!(
        refused.stderr,
        eq(
            "error: Unlink the tags first: team parameters can't be linked to tags yet. Personal's \u{201c}Tier\u{201d}: billing. (parameter_merge_linked)\n"
        )
    );
    Ok(())
}
