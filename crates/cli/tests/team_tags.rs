//! Team tags through the CLI: the path from making one to promoting heap tags into one, and what a run is told when
//! it may not write one.

mod common;

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cli::ToolStart;
use common::{HEAP, Outcome, StandIn, TestCli, TestServer, TestUser, change_heap, cli_in_heap};
use googletest::prelude::*;
use serde_json::{Value, json};

const ONCE: &[&str] = &["work", "run", "--tool", "claude", "--once"];

async fn add_heap(server: &TestServer, user: &TestUser, name: &str) -> Result<()> {
    server
        .send(user, "POST", "/teams/ada/heaps", Some(json!({ "name": name })))
        .await?;
    Ok(())
}

async fn add_heap_tag(server: &TestServer, user: &TestUser, heap: &str, name: &str) -> Result<Value> {
    server
        .send(
            user,
            "POST",
            &format!("/teams/ada/heaps/{heap}/tags"),
            Some(json!({ "name": name })),
        )
        .await
}

/// Adds an item titled `title` in `heap` with the tag `tag`, through the CLI, and returns its key.
async fn add_item(cli: &TestCli, heap: &str, title: &str, tag: &str) -> Result<String> {
    let added = cli
        .run(&["item", "add", title, "--tag", tag, "--heap", heap, "-q"])
        .await;
    assert!(added.exited(0), "item add: {}", added.stderr);
    Ok(added.stdout.trim().to_owned())
}

/// The command a refusal tells to run, as the arguments after `upheap`.
fn confirming_command(stderr: &str) -> Result<Vec<String>> {
    let (_, after) = stderr
        .split_once("To go ahead, run:\n")
        .context("the refusal names a command")?;
    let line = after.lines().next().context("a command line")?.trim();
    let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
    assert_eq!(words.first().map(String::as_str), Some("upheap"));
    Ok(words[1..].to_vec())
}

#[gtest]
#[tokio::test]
async fn the_team_tags_path_steps_1_to_7() -> Result<()> {
    // Arrange: Ada's personal team with heaps A (personal), B and C.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    add_heap(&server, &ada, "Beta").await?;
    add_heap(&server, &ada, "Gamma").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act and assert, step 1: make a team tag; the team's list has it.
    let made = cli
        .run(&[
            "tag",
            "add",
            "security",
            "--team",
            "ada",
            "--description",
            "Security work",
        ])
        .await;
    expect_that!(made.exited(0), eq(true));
    expect_that!(made.stdout, eq("Added team tag \"security\" to ada.\n"));
    let team_list = cli.run(&["tag", "list", "--team", "ada"]).await;
    expect_that!(team_list.stdout, eq("security\n  Security work\n"));

    // Step 2: heap A lists it marked, and an item there takes it by name.
    let in_a = cli.run(&["tag", "list"]).await;
    expect_that!(in_a.stdout, eq("security  team\n  Security work\n"));
    let a_item = add_item(&cli, HEAP, "Audit the logins", "security").await?;

    // Step 3: in B, another item takes it.
    let b_item = add_item(&cli, "ada/beta", "Review the VPN", "security").await?;

    // Step 4: each heap's list by the tag shows its own item.
    let in_b = cli
        .run(&["item", "list", "--tag", "security", "--heap", "ada/beta", "-q"])
        .await;
    let in_a_list = cli.run(&["item", "list", "--tag", "security", "-q"]).await;
    expect_that!(in_b.stdout.trim(), eq(b_item.as_str()));
    expect_that!(in_a_list.stdout.trim(), eq(a_item.as_str()));

    // Step 5: a rename from heap A is the same in every heap.
    let renamed = cli.run(&["tag", "edit", "security", "--name", "security-review"]).await;
    expect_that!(renamed.exited(0), eq(true));
    expect_that!(
        renamed.stdout,
        eq("Renamed team tag \"security\" to \"security-review\" in every heap of ada.\n")
    );
    for (heap, item) in [(HEAP, &a_item), ("ada/beta", &b_item)] {
        let shown = cli.run(&["item", "show", item, "--heap", heap]).await;
        expect_that!(shown.stdout, contains_substring("security-review"));
    }

    // Step 6: an item moved to B keeps the tag.
    let moved = cli.run(&["item", "move", &a_item, "ada/beta"]).await;
    expect_that!(moved.exited(0), eq(true));
    let b_items = cli
        .run(&["item", "list", "--tag", "security-review", "--heap", "ada/beta", "-q"])
        .await;
    expect_that!(b_items.stdout.lines().count(), eq(2));

    // Step 7: heap tags billing in A and B are promoted once the reach is confirmed.
    add_heap_tag(&server, &ada, "personal", "billing").await?;
    add_heap_tag(&server, &ada, "beta", "billing").await?;
    add_item(&cli, HEAP, "Send the invoices", "billing").await?;
    add_item(&cli, "ada/beta", "Chase the payments", "billing").await?;
    let refused = cli.run(&["tag", "promote", "billing", "--heap", HEAP]).await;
    expect_that!(refused.exited(3), eq(true));
    expect_that!(
        refused.stderr,
        contains_substring("Promoting \"billing\" to a team tag of ada merges these heap tags into it:")
    );
    expect_that!(refused.stderr, contains_substring("  personal  billing  1 item\n"));
    expect_that!(refused.stderr, contains_substring("  beta      billing  1 item\n"));
    let as_json = cli.run(&["tag", "promote", "billing", "--heap", HEAP, "--json"]).await;
    expect_that!(as_json.exited(3), eq(true));
    let body: Value = serde_json::from_str(&as_json.stderr)?;
    expect_that!(body["code"], eq("tag_merge_unconfirmed"));
    expect_that!(body["merge_reach"]["heaps"].as_array().map(Vec::len), some(eq(2)));

    let command = confirming_command(&refused.stderr)?;
    let command: Vec<&str> = command.iter().map(String::as_str).collect();
    let promoted = cli.run(&command).await;
    expect_that!(promoted.exited(0), eq(true));
    expect_that!(promoted.stdout, eq("Promoted \"billing\" to a team tag of ada.\n"));
    let listed = cli.run(&["tag", "list", "--json"]).await;
    let listed: Value = serde_json::from_str(&listed.stdout)?;
    expect_that!(
        listed["team_tags"]
            .as_array()
            .map(|tags| tags.iter().any(|tag| tag["name"] == "billing")),
        some(eq(true))
    );
    expect_that!(
        listed["tags"]
            .as_array()
            .map(|tags| tags.iter().any(|tag| tag["name"] == "billing")),
        some(eq(false))
    );
    for heap in [HEAP, "ada/beta"] {
        let by_tag = cli
            .run(&["item", "list", "--tag", "billing", "--heap", heap, "-q"])
            .await;
        expect_that!(by_tag.stdout.lines().count(), eq(1));
    }
    let in_c = cli
        .run(&[
            "item",
            "add",
            "Reconcile",
            "--tag",
            "billing",
            "--heap",
            "ada/gamma",
            "-q",
        ])
        .await;
    expect_that!(in_c.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_agent_in_a_run_given_one_heap_is_refused_a_team_tag_rename_naming_the_other() -> Result<()> {
    // Arrange: the team tag ops is on an item in A and one in B; the run is on a note in A.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    change_heap(&server, &ada, HEAP, json!({ "agents_prepare": true })).await?;
    add_heap(&server, &ada, "Beta").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let made = cli.run(&["tag", "add", "ops", "--team", "ada"]).await;
    assert!(made.exited(0), "{}", made.stderr);
    add_item(&cli, "ada/beta", "Patch the VPN", "ops").await?;
    add_item(&cli, HEAP, "Rotate the keys", "ops").await?;
    server
        .add_item(&ada, HEAP, json!({ "title": "Tidy the alerts" }))
        .await?;
    let agent = cli.agent();
    let outcomes: Arc<Mutex<Vec<Outcome>>> = Arc::default();
    let tool = {
        let outcomes = outcomes.clone();
        StandIn::new(move |start: ToolStart| {
            let (agent, outcomes) = (agent.clone(), outcomes.clone());
            async move {
                let rename = agent
                    .upheap(&start, &["tag", "edit", "ops", "--name", "operations"])
                    .await;
                let tagged = agent
                    .upheap_with_stdin(
                        &start,
                        &[
                            "item",
                            "add",
                            "Check the ops alerts",
                            "--task",
                            "--context-file",
                            "-",
                            "--tag",
                            "ops",
                            "--json",
                        ],
                        "Came up while tidying the alerts.",
                    )
                    .await;
                if let Ok(mut all) = outcomes.lock() {
                    all.extend([rename, tagged]);
                }
                Ok(cli::ToolReport::default())
            }
        })
    };

    // Act
    let ran = cli.run_work(ONCE, &tool).await;

    // Assert
    expect_that!(ran.exited(0), eq(true));
    let outcomes = std::mem::take(&mut *outcomes.lock().map_err(|_| anyhow::anyhow!("poisoned"))?);
    expect_that!(outcomes[0].exited(3), eq(true));
    expect_that!(
        outcomes[0].stderr,
        eq(
            "error: renaming team tag \"ops\" reaches ada/beta, which this run wasn't given.\n\
            hint: ask the person: upheap question request <key> \"Rename the team tag ops to operations\"\n"
        )
    );
    // Putting the team tag on an item the run adds in its own heap works.
    expect_that!(outcomes[1].exited(0), eq(true), "{}", outcomes[1].stderr);
    let added: Value = serde_json::from_str(&outcomes[1].stdout)?;
    expect_that!(added["tags"][0]["name"], eq(&json!("ops")));
    Ok(())
}
