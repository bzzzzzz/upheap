//! `upheap item list`: the ranked list, its flags, saved views, closed items, and the errors on the way.

mod common;

use anyhow::{Context, Result};
use common::{TestCli, TestServer, TestUser};
use googletest::prelude::*;
use serde_json::{Value, json};

const HEAP: &str = "ada/personal";

/// Ada's personal heap with three open items, keyed in the order they were added: a small tagged one with a
/// description, a large one, and one without sizes.
async fn ada_with_items(server: &TestServer) -> Result<(TestUser, Vec<Value>)> {
    let ada = server.register("ada", "Ada").await?;
    let tag = server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/tags",
            Some(json!({ "name": "Infra" })),
        )
        .await?;
    let items = vec![
        server
            .add_item(
                &ada,
                HEAP,
                json!({
                    "title": "Rotate the staging certs", "description": "Before they expire",
                    "effort": "s", "impact": "l", "need": "critical", "tag_ids": [tag["id"]],
                }),
            )
            .await?,
        server
            .add_item(
                &ada,
                HEAP,
                json!({ "title": "Write the migration guide", "effort": "l", "impact": "m" }),
            )
            .await?,
        server
            .add_item(&ada, HEAP, json!({ "title": "Tidy the wiki", "need": "idea" }))
            .await?,
    ];
    Ok((ada, items))
}

fn key(item: &Value) -> Result<&str> {
    item["key"].as_str().context("item has a key")
}

/// The CLI signed in as `user`, working in Ada's personal heap through `UPHEAP_HEAP`.
async fn cli_in_heap(server: &TestServer, user: &TestUser) -> Result<TestCli> {
    let mut cli = TestCli::signed_in(server, user).await?;
    cli.env.heap = Some(HEAP.to_owned());
    Ok(cli)
}

#[gtest]
#[tokio::test]
async fn item_list_shows_the_ranked_list_in_columns() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let listed = cli.run(&["item", "list"]).await;

    // Assert
    let lines: Vec<&str> = listed.stdout.lines().collect();
    expect_that!(lines.len(), eq(3));
    expect_that!(
        lines[0],
        starts_with(format!(
            "1  {}  Rotate the staging certs   open  s/l  ",
            key(&items[0])?
        ))
    );
    expect_that!(lines[0], contains_substring("Infra"));
    expect_that!(lines[0], contains_substring("%)"));
    expect_that!(listed.stdout, contains_substring("Write the migration guide"));
    expect_that!(listed.stdout, contains_substring("Tidy the wiki"));
    expect_that!(listed.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_json_is_the_servers_body_and_quiet_is_the_keys() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let expected = server
        .send(&ada, "GET", "/teams/ada/heaps/personal/items", None)
        .await?;

    // Act
    let json = cli.run(&["item", "list", "--json"]).await;
    let quiet = cli.run(&["item", "list", "--quiet"]).await;

    // Assert
    let printed: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(printed["ranked"], eq(&expected["ranked"]));
    expect_that!(printed["hidden_count"], eq(&expected["hidden_count"]));
    let keys: Vec<&str> = expected["ranked"]
        .as_array()
        .context("ranked")?
        .iter()
        .filter_map(|entry| entry["item"]["key"].as_str())
        .collect();
    expect_that!(quiet.stdout, eq(&format!("{}\n", keys.join("\n"))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_filters_by_the_api_flags_and_by_tag_name() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let large = cli.run(&["item", "list", "-q", "--effort", "l"]).await;
    let small_or_large = cli.run(&["item", "list", "-q", "--effort", "s,l"]).await;
    let tagged = cli.run(&["item", "list", "-q", "--tag", "infra"]).await;
    let ideas = cli.run(&["item", "list", "-q", "--need", "idea"]).await;
    let unknown = cli.run(&["item", "list", "--tag", "ops"]).await;

    expect_that!(large.stdout, eq(&format!("{}\n", key(&items[1])?)));
    expect_that!(small_or_large.stdout.lines().count(), eq(2));
    expect_that!(tagged.stdout, eq(&format!("{}\n", key(&items[0])?)));
    expect_that!(ideas.stdout, eq(&format!("{}\n", key(&items[2])?)));
    expect_that!(
        unknown.stderr,
        eq("error: ada/personal has no tag `ops`; it has Infra\n")
    );
    expect_that!(unknown.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_text_narrows_by_title_and_description() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let by_description = cli.run(&["item", "list", "-q", "EXPIRE"]).await;
    let by_title = cli.run(&["item", "list", "wiki"]).await;
    let nothing = cli.run(&["item", "list", "nothing like this"]).await;
    let with_json = cli.run(&["item", "list", "--json", "wiki"]).await;

    expect_that!(by_description.stdout, eq(&format!("{}\n", key(&items[0])?)));
    expect_that!(by_title.stdout.lines().count(), eq(1));
    expect_that!(by_title.stdout, contains_substring("Tidy the wiki"));
    expect_that!(
        nothing.stdout,
        eq("No items in Personal match \"nothing like this\".\n")
    );
    expect_that!(with_json.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_says_what_its_view_hides_and_shows_it_on_request() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    server.set_state(&ada, HEAP, &items[2], "blocked").await?;
    server.set_state(&ada, HEAP, &items[1], "in_progress").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let default = cli.run(&["item", "list"]).await;
    let shown = cli.run(&["item", "list", "--blocked", "shown"]).await;

    // Assert
    expect_that!(default.stdout, starts_with("Pinned\n"));
    expect_that!(default.stdout, contains_substring("\nRanked\n"));
    expect_that!(
        default.stdout,
        ends_with("\n\n1 item hidden by their state (blocked); show them with --blocked shown.\n")
    );
    expect_that!(default.stdout, not(contains_substring("Tidy the wiki")));
    expect_that!(shown.stdout, contains_substring("\nBlocked\n"));
    expect_that!(shown.stdout, contains_substring("Tidy the wiki"));
    expect_that!(shown.stdout, not(contains_substring("hidden")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_view_sends_the_saved_views_settings() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/views",
            Some(json!({ "name": "Big ones", "view": { "effort": ["l"] } })),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let viewed = cli.run(&["item", "list", "-q", "--view", "big ones"]).await;
    let overridden = cli
        .run(&["item", "list", "-q", "--view", "Big ones", "--effort", "s"])
        .await;
    let unknown = cli.run(&["item", "list", "--view", "Small ones"]).await;

    expect_that!(viewed.stdout, eq(&format!("{}\n", key(&items[1])?)));
    expect_that!(overridden.stdout, eq(&format!("{}\n", key(&items[0])?)));
    expect_that!(
        unknown.stderr,
        contains_substring("has no saved view `Small ones`; it has Big ones")
    );
    expect_that!(unknown.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_closed_lists_closed_items_by_when() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    server.set_state(&ada, HEAP, &items[0], "done").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let closed = cli.run(&["item", "list", "--closed"]).await;
    let quiet = cli.run(&["item", "list", "--closed", "-q"]).await;
    let open = cli.run(&["item", "list", "-q"]).await;
    let with_filter = cli.run(&["item", "list", "--closed", "--effort", "s"]).await;

    expect_that!(closed.stdout, starts_with("Today\n"));
    expect_that!(
        closed.stdout,
        contains_substring(format!("{}  Rotate the staging certs  done", key(&items[0])?))
    );
    expect_that!(quiet.stdout, eq(&format!("{}\n", key(&items[0])?)));
    expect_that!(open.stdout.lines().count(), eq(2));
    expect_that!(with_filter.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_takes_a_bare_heap_slug_and_says_when_none_is_set() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    let bare = cli.run(&["item", "list", "-q", "--heap", "personal"]).await;
    let none = cli.run(&["item", "list"]).await;

    expect_that!(bare.stdout.lines().count(), eq(3));
    expect_that!(
        none.stderr,
        contains_substring("no heap set: pick one with `upheap heap use <team/heap>`")
    );
    expect_that!(none.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_refusal_prints_the_servers_message_and_code() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let missing = cli.run(&["item", "list", "--heap", "ada/nope"]).await;
    let mismatch = cli.run(&["item", "list", "--sort", "parameter"]).await;
    let mismatch_json = cli.run(&["item", "list", "--sort", "parameter", "--json"]).await;

    expect_that!(missing.stderr, starts_with("error: "));
    expect_that!(
        missing.stderr,
        contains_substring("(not_found)\nhint: `upheap heap list`")
    );
    expect_that!(missing.exited(4), eq(true));
    expect_that!(mismatch.stderr, contains_substring("(sort_parameter_mismatch)"));
    expect_that!(mismatch.exited(3), eq(true));
    let problem: Value = serde_json::from_str(&mismatch_json.stderr)?;
    expect_that!(problem["code"].as_str(), some(eq("sort_parameter_mismatch")));
    expect_that!(mismatch_json.stdout, eq(""));
    Ok(())
}
