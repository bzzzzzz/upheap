//! `upheap heap add`, `list`, `use`, `show`, `edit` and `weights`.

mod common;

use anyhow::Result;
use common::{TestCli, TestServer, TestUser};
use googletest::prelude::*;
use serde_json::json;

/// Ada with her personal team, and an `infra` team with heaps `backlog` and `ops`; her personal team has a
/// `backlog` too, and an archived `old`.
async fn ada_with_teams(server: &TestServer) -> Result<TestUser> {
    let ada = server.register("ada", "Ada").await?;
    server
        .send(
            &ada,
            "POST",
            "/teams",
            Some(json!({ "name": "Infrastructure", "slug": "infra" })),
        )
        .await?;
    for (team, slug) in [
        ("infra", "backlog"),
        ("infra", "ops"),
        ("ada", "backlog"),
        ("ada", "old"),
    ] {
        server
            .send(
                &ada,
                "POST",
                &format!("/teams/{team}/heaps"),
                Some(json!({ "name": slug, "slug": slug })),
            )
            .await?;
    }
    server.send(&ada, "POST", "/teams/ada/heaps/old/archive", None).await?;
    Ok(ada)
}

#[gtest]
#[tokio::test]
async fn heap_list_shows_each_team_with_its_heaps() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    server
        .add_item(&ada, "infra/ops", json!({ "title": "Patch the kernel" }))
        .await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let human = cli.run(&["heap", "list"]).await;
    let quiet = cli.run(&["heap", "list", "-q"]).await;
    let json = cli.run(&["heap", "list", "--json"]).await;

    // Assert
    expect_that!(
        human.stdout,
        eq("Ada (ada, personal)\n  \
              ada/personal  Personal  0 open\n  \
              ada/backlog   backlog   0 open\n  \
              ada/old       old       0 open  archived\n\
            \n\
            Infrastructure (infra)\n  \
              infra/backlog  backlog  0 open\n  \
              infra/ops      ops      1 open\n")
    );
    expect_that!(
        quiet.stdout,
        eq("ada/personal\nada/backlog\nada/old\ninfra/backlog\ninfra/ops\n")
    );
    let me: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(me["user"]["username"].as_str(), some(eq("ada")));
    expect_that!(json.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_sets_the_default_which_heap_list_marks() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let used = cli.run(&["heap", "use", "ops"]).await;
    let listed = cli.run(&["heap", "list"]).await;

    // Assert
    expect_that!(
        used.stdout,
        // The server came from UPHEAP_SERVER and the file had none, so it's saved with the heap.
        eq(&format!(
            "Commands now work in infra/ops unless told otherwise.\nThe default server is now {} too, the server infra/ops is on.\n",
            server.url
        ))
    );
    expect_that!(used.exited(0), eq(true));
    expect_that!(
        cli.config_text(),
        contains_substring(format!("server = \"{}\"", server.url))
    );
    expect_that!(cli.config_text(), contains_substring("heap = \"infra/ops\""));
    expect_that!(listed.stdout, contains_substring("* infra/ops      ops      0 open"));
    expect_that!(listed.stdout, contains_substring("  infra/backlog  backlog  0 open"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_refuses_a_slug_two_teams_have() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    let used = cli.run(&["heap", "use", "backlog"]).await;

    expect_that!(
        used.stderr,
        eq("error: more than one of your teams has a heap `backlog`: say which with ada/backlog or infra/backlog\n")
    );
    expect_that!(used.exited(3), eq(true));
    expect_that!(cli.config_text(), eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_refuses_a_heap_the_caller_doesnt_have() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    let unknown = cli.run(&["heap", "use", "infra/nope"]).await;
    let malformed = cli.run(&["heap", "use", "a/b/c"]).await;

    expect_that!(unknown.stderr, contains_substring("you have no heap `infra/nope`"));
    expect_that!(unknown.exited(3), eq(true));
    expect_that!(malformed.exited(2), eq(true));
    expect_that!(cli.config_text(), eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_use_with_another_server_saves_the_server_with_the_heap() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    cli.env.server = None;

    let used = cli.run(&["heap", "use", "ops", "--server", &server.url]).await;
    let again = cli.run(&["heap", "use", "ada/personal", "--server", &server.url]).await;

    expect_that!(
        used.stdout,
        eq(&format!(
            "Commands now work in infra/ops unless told otherwise.\nThe default server is now {} too, the server infra/ops is on.\n",
            server.url
        ))
    );
    expect_that!(
        again.stdout,
        eq("Commands now work in ada/personal unless told otherwise.\n")
    );
    expect_that!(
        cli.config_text(),
        contains_substring(format!("server = \"{}\"", server.url))
    );
    expect_that!(cli.config_text(), contains_substring("heap = \"ada/personal\""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_show_prints_the_context_indented_or_how_to_write_one() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    cli.env.heap = Some("ada/personal".to_owned());
    let ops = server.send(&ada, "GET", "/teams/infra/heaps/ops", None).await?;
    server
        .send_with(
            &ada,
            "PATCH",
            "/teams/infra/heaps/ops",
            Some(json!({ "context": "Docs: the wiki\r\n\tReview on GitHub" })),
            &[("if-match", &ops["version"].to_string())],
        )
        .await?;

    // Act
    let written = cli.run(&["heap", "show", "infra/ops"]).await;
    let empty = cli.run(&["heap", "show", "infra/backlog"]).await;
    let default = cli.run(&["heap", "show"]).await;
    let archived = cli.run(&["heap", "show", "ada/old"]).await;
    let json = cli.run(&["heap", "show", "ada/old", "--json"]).await;
    let quiet = cli.run(&["heap", "show", "ada/old", "-q"]).await;
    let unknown = cli.run(&["heap", "show", "infra/nope"]).await;

    // Assert
    expect_that!(
        written.stdout,
        eq(
            "ops · Infrastructure · agents prepare new notes: off\n\nWeights\n  Need      10\n  Age       10\n  Deadline  10\n  Value     10\n\nContext\n  Docs: the wiki\n      Review on GitHub\n"
        )
    );
    expect_that!(
        empty.stdout,
        eq(
            "backlog · Infrastructure · agents prepare new notes: off\n\nWeights\n  Need      10\n  Age       10\n  Deadline  10\n  Value     10\n\nContext\n  None. Write one with: upheap heap edit infra/backlog --context-file <file>\n"
        )
    );
    expect_that!(
        default.stdout,
        starts_with(
            "Personal · Ada · agents prepare new notes: off\n\nWeights\n  Need      10\n  Age       10\n  Deadline  10\n  Value     10\n\nContext\n"
        )
    );
    expect_that!(
        archived.stdout,
        starts_with("old · Ada · archived · agents prepare new notes: off\n")
    );
    let heap: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(heap["slug"].as_str(), some(eq("old")));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(unknown.exited(3), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_show_says_whether_agents_prepare_new_notes() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let ops = server.send(&ada, "GET", "/teams/infra/heaps/ops", None).await?;
    server
        .send_with(
            &ada,
            "PATCH",
            "/teams/infra/heaps/ops",
            Some(json!({ "agents_prepare": true })),
            &[("if-match", &ops["version"].to_string())],
        )
        .await?;

    // Act
    let on = cli.run(&["heap", "show", "infra/ops"]).await;
    let off = cli.run(&["heap", "show", "infra/backlog"]).await;
    let json = cli.run(&["heap", "show", "infra/ops", "--json"]).await;

    // Assert
    expect_that!(
        on.stdout,
        starts_with("ops · Infrastructure · agents prepare new notes: on\n")
    );
    expect_that!(
        off.stdout,
        starts_with("backlog · Infrastructure · agents prepare new notes: off\n")
    );
    let heap: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(heap["agents_prepare"], eq(&json!(true)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_edit_writes_the_context_from_a_file_or_stdin_and_clears_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("context.txt");
    std::fs::write(&path, "Docs: the wiki\r\n\tReview on GitHub\n")?;
    let path = path.to_string_lossy().into_owned();

    // Act
    let from_file = cli.run(&["heap", "edit", "infra/ops", "--context-file", &path]).await;
    let shown = cli.run(&["heap", "show", "infra/ops"]).await;
    let again = cli.run(&["heap", "edit", "infra/ops", "--context-file", &path]).await;
    cli.stdin = b"From stdin\n".to_vec();
    let from_stdin = cli
        .run(&["heap", "edit", "infra/ops", "--context-file", "-", "--json"])
        .await;
    let cleared = cli.run(&["heap", "edit", "infra/ops", "--no-context"]).await;
    let quiet = cli
        .run(&["heap", "edit", "infra/ops", "--context-file", &path, "-q"])
        .await;
    let stored = server.send(&ada, "GET", "/teams/infra/heaps/ops", None).await?;

    // Assert
    expect_that!(from_file.stdout, eq("Updated the context of infra/ops.\n"));
    expect_that!(
        shown.stdout,
        eq(
            "ops · Infrastructure · agents prepare new notes: off\n\nWeights\n  Need      10\n  Age       10\n  Deadline  10\n  Value     10\n\nContext\n  Docs: the wiki\n      Review on GitHub\n"
        )
    );
    expect_that!(again.stdout, eq("No change to infra/ops.\n"));
    let heap: serde_json::Value = serde_json::from_str(&from_stdin.stdout)?;
    expect_that!(heap["context"].as_str(), some(eq("From stdin")));
    expect_that!(cleared.stdout, eq("Updated the context of infra/ops.\n"));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(
        stored["context"].as_str(),
        some(eq("Docs: the wiki\r\n\tReview on GitHub"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_edit_does_nothing_without_a_flag_and_refuses_both_or_an_unreadable_file() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let none = cli.run(&["heap", "edit", "infra/ops"]).await;
    let both = cli
        .run(&["heap", "edit", "infra/ops", "--context-file", "x.txt", "--no-context"])
        .await;
    let missing = cli
        .run(&[
            "heap",
            "edit",
            "infra/ops",
            "--context-file",
            "/nonexistent/context.txt",
        ])
        .await;
    let archived = cli.run(&["heap", "edit", "ada/old", "--no-context"]).await;

    // Assert
    expect_that!(none.exited(0), eq(true));
    expect_that!(
        none.stdout,
        eq("Nothing to change: give --context-file or --no-context.\n")
    );
    expect_that!(both.exited(2), eq(true));
    expect_that!(missing.exited(2), eq(true));
    expect_that!(missing.stderr, contains_substring("/nonexistent/context.txt"));
    expect_that!(archived.exited(0), eq(false));
    expect_that!(archived.stderr, contains_substring("archived"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_add_makes_a_heap_in_the_personal_team_or_the_named_one() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let cli = TestCli::signed_in(&server, &ada).await?;

    // Act
    let personal = cli.run(&["heap", "add", "Road Map"]).await;
    let named = cli
        .run(&["heap", "add", "Plans", "--team", "infra", "--slug", "plan", "-q"])
        .await;
    let json = cli.run(&["heap", "add", "Road Map", "--json"]).await;
    let listed = cli.run(&["heap", "list", "-q"]).await;

    // Assert
    expect_that!(personal.stdout, eq("Added heap ada/road-map.\n"));
    expect_that!(personal.exited(0), eq(true));
    expect_that!(named.stdout, eq("infra/plan\n"));
    let heap: serde_json::Value = serde_json::from_str(&json.stdout)?;
    // The same name again gets the next free slug from the server.
    expect_that!(heap["slug"].as_str(), some(eq("road-map-2")));
    expect_that!(heap["name"].as_str(), some(eq("Road Map")));
    for handle in ["ada/road-map\n", "ada/road-map-2\n", "infra/plan\n"] {
        expect_that!(listed.stdout, contains_substring(handle));
    }
    expect_that!(cli.config_text(), eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_add_reports_what_the_server_refuses() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let grace = server.register("grace", "Grace").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let outsider = TestCli::signed_in(&server, &grace).await?;

    // Act
    let taken = cli
        .run(&["heap", "add", "Ops again", "--team", "infra", "--slug", "ops"])
        .await;
    let malformed = cli.run(&["heap", "add", "Plans", "--slug", "Not A Slug"]).await;
    let blank = cli.run(&["heap", "add", " "]).await;
    let not_mine = outsider.run(&["heap", "add", "Plans", "--team", "infra"]).await;
    let no_name = cli.run(&["heap", "add"]).await;

    // Assert
    expect_that!(taken.stderr, contains_substring("handle_taken"));
    expect_that!(taken.exited(3), eq(true));
    expect_that!(malformed.stderr, contains_substring("invalid_handle"));
    expect_that!(malformed.exited(3), eq(true));
    expect_that!(blank.exited(3), eq(true));
    expect_that!(not_mine.exited(4), eq(true));
    expect_that!(no_name.exited(2), eq(true));
    let heaps = cli.run(&["heap", "list", "-q"]).await;
    expect_that!(
        heaps.stdout,
        eq("ada/personal\nada/backlog\nada/old\ninfra/backlog\ninfra/ops\n")
    );
    Ok(())
}

/// Ada's personal heap with two parameters, `Area` and `Customer`, both at the default weight.
async fn ada_with_parameters(server: &TestServer) -> Result<(TestUser, serde_json::Value)> {
    let ada = server.register("ada", "Ada").await?;
    let area = server.add_parameter(&ada, "Area", &["Backend"], &[]).await?;
    server.add_parameter(&ada, "Customer", &["Acme"], &[]).await?;
    Ok((ada, area))
}

/// The weights of Ada's personal heap, `[need, age, deadline, value]`, and of its parameters in use as
/// `(name, weight)`, as the API has them.
async fn stored_weights(server: &TestServer, ada: &TestUser) -> Result<(Vec<i64>, Vec<(String, i64)>)> {
    let heap = server.send(ada, "GET", "/teams/ada/heaps/personal", None).await?;
    let list = server
        .send(ada, "GET", "/teams/ada/heaps/personal/parameters", None)
        .await?;
    let heap_weights = ["need", "age", "deadline", "value"]
        .iter()
        .map(|name| heap["weights"][name].as_i64().unwrap_or(-1))
        .collect();
    let parameters = list["parameters"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap_or_default().to_owned(),
                p["weight"].as_i64().unwrap_or(-1),
            )
        })
        .collect();
    Ok((heap_weights, parameters))
}

fn untouched_weights() -> (Vec<i64>, Vec<(String, i64)>) {
    (
        vec![10, 10, 10, 10],
        vec![("Area".to_owned(), 10), ("Customer".to_owned(), 10)],
    )
}

#[gtest]
#[tokio::test]
async fn heap_show_prints_the_weights_with_the_parameters_in_use() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, area) = ada_with_parameters(&server).await?;
    let mut cli = TestCli::signed_in(&server, &ada).await?;
    cli.env.heap = Some("ada/personal".to_owned());
    let legacy = server.add_parameter(&ada, "Legacy", &["Old"], &[]).await?;
    let legacy_path = format!(
        "/teams/ada/heaps/personal/parameters/{}/archive",
        legacy["id"].as_str().unwrap_or_default()
    );
    server.send(&ada, "POST", &legacy_path, None).await?;
    let heap = server.send(&ada, "GET", "/teams/ada/heaps/personal", None).await?;
    server
        .send_with(
            &ada,
            "PATCH",
            "/teams/ada/heaps/personal/weights",
            Some(json!({ "age": 0, "parameters": [{ "id": area["id"], "weight": 20, "version": area["version"] }] })),
            &[("if-match", &heap["version"].to_string())],
        )
        .await?;

    // Act
    let shown = cli.run(&["heap", "show"]).await;
    let json = cli.run(&["heap", "show", "--json"]).await;

    // Assert
    expect_that!(
        shown.stdout,
        starts_with(
            "Personal · Ada · agents prepare new notes: off\n\nWeights\n  Need      10\n  Age       0 (off)\n  Deadline  10\n  Value     10\n  Area      20       parameter\n  Customer  10       parameter\n\nContext\n"
        )
    );
    expect_that!(shown.stdout, not(contains_substring("Legacy")));
    let body: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["weights"]["age"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_changes_heap_and_parameter_weights_in_one_save() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_parameters(&server).await?;
    let cli = common::cli_in_heap(&server, &ada).await?;

    // Act
    let changed = cli
        .run(&[
            "heap",
            "weights",
            "--need",
            "30",
            "--value",
            "0",
            "--parameter",
            "area=40",
        ])
        .await;
    let stored = stored_weights(&server, &ada).await?;

    // Assert
    expect_that!(
        changed.stdout,
        eq(
            "Updated the weights of ada/personal.\n\nWeights\n  Need      30\n  Age       10\n  Deadline  10\n  Value     0 (off)\n  Area      40       parameter\n  Customer  10       parameter\n"
        )
    );
    expect_that!(stored.0, eq(&vec![30, 10, 10, 0]));
    expect_that!(
        stored.1,
        eq(&vec![("Area".to_owned(), 40), ("Customer".to_owned(), 10)])
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_changes_only_a_parameter_when_only_one_is_given() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_parameters(&server).await?;
    let cli = common::cli_in_heap(&server, &ada).await?;

    // Act
    let changed = cli.run(&["heap", "weights", "--parameter", "Customer=5"]).await;
    let stored = stored_weights(&server, &ada).await?;

    // Assert
    expect_that!(changed.stdout, starts_with("Updated the weights of ada/personal.\n"));
    expect_that!(stored.0, eq(&vec![10, 10, 10, 10]));
    expect_that!(stored.1, eq(&vec![("Area".to_owned(), 10), ("Customer".to_owned(), 5)]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_says_no_change_when_the_weights_are_already_so() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_parameters(&server).await?;
    let cli = common::cli_in_heap(&server, &ada).await?;

    // Act
    let same = cli.run(&["heap", "weights", "--need", "10"]).await;
    let quiet = cli.run(&["heap", "weights", "--need", "20", "-q"]).await;

    // Assert
    expect_that!(same.exited(0), eq(true));
    expect_that!(same.stdout, eq("No change to ada/personal.\n"));
    expect_that!(quiet.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_without_a_flag_does_nothing() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_parameters(&server).await?;
    let cli = common::cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli.run(&["heap", "weights"]).await;
    let quiet = cli.run(&["heap", "weights", "-q"]).await;
    let json = cli.run(&["heap", "weights", "--json"]).await;

    // Assert
    expect_that!(human.exited(0), eq(true));
    expect_that!(
        human.stdout,
        eq("Nothing to change: give --need, --age, --deadline, --value or --parameter.\n")
    );
    expect_that!(quiet.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(""));
    expect_that!(json.exited(0), eq(true));
    expect_that!(json.stdout, eq(""));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_refuses_bad_values_and_unknown_or_conflicting_parameters() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_parameters(&server).await?;
    let cli = common::cli_in_heap(&server, &ada).await?;

    // Act
    let too_big = cli.run(&["heap", "weights", "--need", "101"]).await;
    let not_a_number = cli.run(&["heap", "weights", "--need", "lots"]).await;
    let no_equals = cli.run(&["heap", "weights", "--parameter", "Area"]).await;
    let unknown = cli.run(&["heap", "weights", "--parameter", "Nope=5"]).await;
    let conflict = cli
        .run(&["heap", "weights", "--parameter", "Area=10", "--parameter", "area=20"])
        .await;
    let after_refusals = stored_weights(&server, &ada).await?;
    let repeated = cli
        .run(&["heap", "weights", "--parameter", "Area=15", "--parameter", "area=15"])
        .await;
    let after_repeat = stored_weights(&server, &ada).await?;

    // Assert
    expect_that!(too_big.exited(2), eq(true));
    expect_that!(not_a_number.exited(2), eq(true));
    expect_that!(no_equals.exited(2), eq(true));
    expect_that!(
        no_equals.stderr,
        contains_substring("--parameter takes NAME=N, such as --parameter Area=20; got `Area`")
    );
    expect_that!(unknown.exited(3), eq(true));
    expect_that!(unknown.stderr, contains_substring("Area"));
    expect_that!(unknown.stderr, contains_substring("Customer"));
    expect_that!(conflict.exited(2), eq(true));
    expect_that!(
        conflict.stderr,
        contains_substring("--parameter names Area twice; give it once")
    );
    expect_that!(after_refusals, eq(&untouched_weights()));
    expect_that!(repeated.exited(0), eq(true));
    expect_that!(
        after_repeat.1,
        eq(&vec![("Area".to_owned(), 15), ("Customer".to_owned(), 10)])
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_json_prints_the_response_body() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let (ada, _) = ada_with_parameters(&server).await?;
    let cli = common::cli_in_heap(&server, &ada).await?;

    // Act
    let json = cli.run(&["heap", "weights", "--need", "45", "--json"]).await;

    // Assert
    let body: serde_json::Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["heap"]["weights"]["need"], eq(&json!(45)));
    expect_that!(body["parameters"].as_array().map(Vec::len), some(eq(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heap_weights_by_a_member_is_refused() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = ada_with_teams(&server).await?;
    let bob = server.register("bob", "Bob").await?;
    let me = server.send(&bob, "GET", "/me", None).await?;
    let join = json!({ "user_id": me["user"]["id"] });
    server.send(&ada, "POST", "/teams/infra/members", Some(join)).await?;
    let cli = TestCli::signed_in(&server, &bob).await?;

    // Act
    let refused = cli.run(&["heap", "weights", "infra/ops", "--need", "50"]).await;
    let stored = server.send(&ada, "GET", "/teams/infra/heaps/ops", None).await?;

    // Assert
    expect_that!(refused.exited(4), eq(true));
    expect_that!(refused.stdout, eq(""));
    expect_that!(stored["weights"]["need"], eq(&json!(10)));
    Ok(())
}
