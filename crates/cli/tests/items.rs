//! `upheap item list`: the ranked list, its flags, saved views, closed items, and the errors on the way.

mod common;

use anyhow::{Context, Result};
use common::{HEAP, TestCli, TestServer, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

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
            "1  {}  note  Rotate the staging certs     open    s/l  ",
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
async fn item_list_gives_a_blocked_items_reason_in_its_state_cell() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    server
        .edit_item(
            &ada,
            HEAP,
            key(&items[2])?,
            json!({ "state": "blocked", "blocked_reason": "Waits for the vendor's quote" }),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let shown = cli.run(&["item", "list", "--blocked", "shown"]).await;

    // Assert
    expect_that!(
        shown.stdout,
        contains_substring("blocked: Waits for the vendor's quote")
    );
    Ok(())
}

/// Ada's items, the first a task with two steps, one ticked, and a question open on it, and the second sent to an
/// agent. Returns the two keys.
async fn ada_with_marked_items(server: &TestServer) -> Result<(common::TestUser, String, String)> {
    let (ada, items) = ada_with_items(server).await?;
    let cli = cli_in_heap(server, &ada).await?;
    let task = key(&items[0])?.to_owned();
    let sent = key(&items[1])?.to_owned();
    server.edit_item(&ada, HEAP, &task, json!({ "kind": "task" })).await?;
    for title in ["One", "Two"] {
        cli.run(&["step", "add", &task, title]).await;
    }
    cli.run(&["step", "tick", &format!("{task}.1")]).await;
    cli.run(&["question", "ask", &task, "Which CA?"]).await;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{sent}/turn"),
            Some(json!({ "reach": "all_the_way" })),
        )
        .await?;
    Ok((ada, task, sent))
}

/// Where `needle` starts in the line that has `key`.
fn column_of(text: &str, key: &str, needle: &str) -> Option<usize> {
    text.lines().find(|line| line.contains(key))?.find(needle)
}

#[gtest]
#[tokio::test]
async fn item_list_rows_show_the_kind_the_steps_and_who_the_item_waits_for() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, task, sent) = ada_with_marked_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let listed = cli.run(&["item", "list"]).await;
    let across = cli.run(&["item", "list", "--all-heaps"]).await;

    // Assert
    for (form, out) in [("one heap", &listed), ("all heaps", &across)] {
        expect_that!(out.exited(0), eq(true), "{form}");
        let task_line = common::lines_with(&out.stdout, &task).join("");
        let sent_line = common::lines_with(&out.stdout, &sent).join("");
        expect_that!(
            task_line,
            contains_regex(format!(
                "{task}  .*task  Rotate the staging certs +1/2  open +waits for you  s/l"
            )),
            "{form}"
        );
        expect_that!(
            sent_line,
            contains_regex(format!(
                "{sent}  .*note  Write the migration guide +open · with an agent +l/m"
            )),
            "{form}"
        );
        expect_that!(sent_line, not(contains_substring("waits for you")), "{form}");
        // The state cell starts at the same column on every row, whatever the cells before it hold.
        expect_that!(
            column_of(&out.stdout, &task, "open"),
            eq(column_of(&out.stdout, &sent, "open")),
            "{form}"
        );
    }
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
    let missing_json = cli.run(&["item", "list", "--heap", "ada/nope", "--json"]).await;

    expect_that!(missing.stderr, starts_with("error: "));
    expect_that!(
        missing.stderr,
        contains_substring("(not_found)\nhint: `upheap heap list`")
    );
    expect_that!(missing.exited(4), eq(true));
    let problem: Value = serde_json::from_str(&missing_json.stderr)?;
    expect_that!(problem["code"].as_str(), some(eq("not_found")));
    expect_that!(missing_json.stdout, eq(""));
    expect_that!(missing_json.exited(4), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reader_that_stops_early_isnt_an_error() -> Result<()> {
    // As `upheap item list -q | head -1`: the pipe closes after the first line.
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let listed = cli
        .run_into(&["item", "list", "-q"], Box::new(common::ClosedPipe))
        .await;

    expect_that!(listed.stderr, eq(""));
    expect_that!(listed.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn control_characters_in_server_text_are_shown_not_run() -> Result<()> {
    // Arrange: a title with a clear-screen escape and a newline that would forge a row.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let item = server
        .add_item(&ada, HEAP, json!({ "title": "Pay\u{1b}[2J the\n1  zzzzz  forged row" }))
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli.run(&["item", "list"]).await;
    let json = cli.run(&["item", "list", "--json"]).await;

    // Assert
    expect_that!(human.stdout, not(contains_substring("\u{1b}")));
    expect_that!(human.stdout.lines().count(), eq(1));
    expect_that!(
        human.stdout,
        contains_substring("Pay\u{FFFD}[2J the\u{FFFD}1  zzzzz  forged row")
    );
    let listed: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(listed["ranked"][0]["item"]["title"], eq(&item["title"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn json_on_a_terminal_escapes_c1_characters_and_piped_json_is_as_it_came() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    server
        .add_item(&ada, HEAP, json!({ "title": "Pay\u{9b}2J the bills" }))
        .await?;
    let mut cli = cli_in_heap(&server, &ada).await?;

    let piped = cli.run(&["item", "list", "--json"]).await;
    cli.env.stdout_is_terminal = true;
    let on_terminal = cli.run(&["item", "list", "--json"]).await;

    expect_that!(piped.stdout, contains_substring("Pay\u{9b}2J"));
    expect_that!(on_terminal.stdout, not(contains_substring("\u{9b}")));
    expect_that!(on_terminal.stdout, contains_substring("Pay\\u009b2J"));
    let read: Value = serde_json::from_str(&on_terminal.stdout)?;
    let as_it_came: Value = serde_json::from_str(&piped.stdout)?;
    expect_that!(read, eq(&as_it_came));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sort_parameter_doesnt_go_with_another_sort() -> Result<()> {
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    let listed = cli
        .run(&["item", "list", "--sort", "due", "--sort-parameter", "Customer"])
        .await;

    expect_that!(
        listed.stderr,
        eq(
            "error: --sort-parameter sorts by a custom parameter, so it doesn't go with --sort due; leave out one of them\n"
        )
    );
    expect_that!(listed.exited(2), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn sorting_by_a_parameter_needs_one_from_the_flags_or_the_view() -> Result<()> {
    // Arrange: a parameter whose options rank the large item first, and a view sorted by it.
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let parameter = server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/parameters",
            Some(json!({ "name": "Customer", "options": ["Big", "Small"] })),
        )
        .await?;
    let big = parameter["options"][0]["id"].clone();
    let key_of_large = key(&items[1])?;
    let large_version = items[1]["version"].to_string();
    server
        .send_with(
            &ada,
            "PATCH",
            &format!("/teams/ada/heaps/personal/items/{key_of_large}"),
            Some(json!({ "parameter_values": { parameter["id"].as_str().context("id")?: big } })),
            &[("if-match", &large_version)],
        )
        .await?;
    server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/views",
            Some(json!({ "name": "By customer", "view": { "sort": "parameter", "sort_parameter": parameter["id"] } })),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let with_view = cli
        .run(&["item", "list", "-q", "--view", "By customer", "--sort", "parameter"])
        .await;
    let alone = cli.run(&["item", "list", "--sort", "parameter"]).await;

    // Assert
    expect_that!(with_view.exited(0), eq(true));
    expect_that!(with_view.stdout.lines().next(), some(eq(key_of_large)));
    expect_that!(
        alone.stderr,
        eq("error: --sort parameter needs the parameter to sort by: add --sort-parameter <name>\n")
    );
    expect_that!(alone.exited(2), eq(true));
    Ok(())
}

/// Ada with her three personal items, and a second heap `ada/ops` with one more. Returns the keys the server's
/// list across heaps gives, in order, as `team/heap/key`.
async fn ada_with_two_heaps(server: &TestServer) -> Result<(common::TestUser, Vec<String>)> {
    let (ada, _) = ada_with_items(server).await?;
    server
        .send(&ada, "POST", "/teams/ada/heaps", Some(json!({ "name": "Ops" })))
        .await?;
    server
        .add_item(
            &ada,
            "ada/ops",
            json!({ "title": "Patch the hosts", "need": "critical" }),
        )
        .await?;
    let across = server.send(&ada, "GET", "/me/items", None).await?;
    let names = across["ranked"]
        .as_array()
        .context("ranked")?
        .iter()
        .map(|row| {
            format!(
                "{}/{}/{}",
                row["heap"]["team_slug"].as_str().unwrap_or_default(),
                row["heap"]["slug"].as_str().unwrap_or_default(),
                row["entry"]["item"]["key"].as_str().unwrap_or_default()
            )
        })
        .collect();
    Ok((ada, names))
}

#[gtest]
#[tokio::test]
async fn item_list_all_heaps_lists_every_heap_in_one_ranked_list_with_no_heap_set() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, names) = ada_with_two_heaps(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let listed = cli.run(&["item", "list", "--all-heaps"]).await;

    // Assert
    let lines: Vec<&str> = listed.stdout.lines().collect();
    expect_that!(lines.len(), eq(4));
    for (index, (line, name)) in lines.iter().zip(&names).enumerate() {
        let (heap, key) = name.rsplit_once('/').context("name")?;
        expect_that!(
            *line,
            starts_with(format!("{}  {key}  {heap}  ", index + 1)),
            "line {index}"
        );
    }
    expect_that!(listed.stdout, contains_substring("Patch the hosts"));
    expect_that!(listed.stdout, contains_substring("ada/ops"));
    expect_that!(listed.stderr, eq(""));
    expect_that!(listed.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_all_heaps_quiet_prints_team_heap_key_in_order_and_json_is_the_body() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, names) = ada_with_two_heaps(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let quiet = cli.run(&["item", "list", "--all-heaps", "-q"]).await;
    let json = cli.run(&["item", "list", "--all-heaps", "--json"]).await;

    // Assert
    expect_that!(quiet.stdout, eq(&format!("{}\n", names.join("\n"))));
    let printed: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(printed["count"], eq(&json!(4)));
    expect_that!(printed["ranked"].as_array().map(Vec::len), some(eq(4)));
    // A name from `-q` is one every item command takes.
    let shown = cli.run(&["item", "show", &names[0]]).await;
    expect_that!(shown.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_all_heaps_narrows_by_text_and_by_the_view_flags() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_two_heaps(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let by_text = cli.run(&["item", "list", "--all-heaps", "patch"]).await;
    let nothing = cli.run(&["item", "list", "--all-heaps", "zebra"]).await;
    let critical = cli
        .run(&["item", "list", "--all-heaps", "--need", "critical", "-q"])
        .await;

    // Assert
    expect_that!(by_text.stdout.lines().count(), eq(1));
    expect_that!(by_text.stdout, contains_substring("Patch the hosts"));
    expect_that!(nothing.stdout, eq("No items in your heaps match \"zebra\".\n"));
    expect_that!(critical.stdout.lines().count(), eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_all_heaps_says_so_when_there_is_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let listed = cli.run(&["item", "list", "--all-heaps"]).await;

    // Assert
    expect_that!(listed.stdout, eq("No items in your heaps.\n"));
    expect_that!(listed.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_list_all_heaps_doesnt_go_with_the_one_heap_flags_and_sends_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_items(&server).await?;
    let cli = cli_in_heap(&server, &ada).await?;

    for flags in [
        &["--heap", "ada/personal"][..],
        &["--tag", "Infra"],
        &["--closed"],
        &["--view", "mine"],
        &["--sort", "due"],
        &["--sort-parameter", "Customer"],
    ] {
        // Act
        let mut args = vec!["item", "list", "--all-heaps"];
        args.extend_from_slice(flags);
        let listed = cli.run(&args).await;

        // Assert
        expect_that!(listed.exited(2), eq(true), "{flags:?}");
        expect_that!(listed.stdout, eq(""), "{flags:?}");
        expect_that!(listed.stderr, contains_substring("--all-heaps"), "{flags:?}");
    }
    Ok(())
}
