//! `upheap question ask | request` and the Questions section of `upheap item show`.

mod common;

use anyhow::Result;
use common::{HEAP, TestCli, TestServer, TestUser, ada_with_items, cli_in_heap, key};
use googletest::prelude::*;
use serde_json::{Value, json};

/// A server with Ada and her items, the first of which is a task with two steps. Returns the first item's key too.
async fn with_task() -> Result<(TestServer, TestUser, Vec<Value>, String)> {
    let server = TestServer::start().await?;
    let (ada, items) = ada_with_items(&server).await?;
    let task = key(&items[0])?.to_owned();
    server.edit_item(&ada, HEAP, &task, json!({ "kind": "task" })).await?;
    for title in ["One", "Two"] {
        server
            .send(
                &ada,
                "POST",
                &format!("/teams/ada/heaps/personal/items/{task}/steps"),
                Some(json!({ "title": title })),
            )
            .await?;
    }
    Ok((server, ada, items, task))
}

/// Replies to question `number` of `task` through the API.
async fn reply(server: &TestServer, user: &TestUser, task: &str, number: u32, reply: Value) -> Result<Value> {
    server
        .send_with(
            user,
            "PATCH",
            &format!("/teams/ada/heaps/personal/items/{task}/questions/{number}"),
            Some(json!({ "reply": reply })),
            &[("if-match", "1")],
        )
        .await
}

#[gtest]
#[tokio::test]
async fn question_ask_says_what_it_asked_and_prints_its_name_with_quiet() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli.run(&["question", "ask", &task, "Which CA?"]).await;
    let quiet = cli
        .run(&["question", "ask", &task, "And the key size?", "--quiet"])
        .await;
    let json = cli.run(&["question", "ask", &task, "Anything else?", "--json"]).await;

    // Assert
    expect_that!(human.stdout, eq(&format!("Asked {task}.q1: Which CA?\n")));
    expect_that!(human.stderr, eq(""));
    expect_that!(human.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(&format!("{task}.q2\n")));
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["questions"][2]["text"], eq("Anything else?"));
    expect_that!(body["questions"][2]["number"], eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_ask_takes_choices_many_and_a_step_by_name_or_number() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let one = cli
        .run(&[
            "question",
            "ask",
            &task,
            "Which CA?",
            "--choice",
            "Internal",
            "--choice",
            "Let's Encrypt",
            "--step",
            &format!("{task}.2"),
        ])
        .await;
    let many = cli
        .run(&[
            "question",
            "ask",
            &task,
            "Which hosts?",
            "--choice",
            "a",
            "--choice",
            "b",
            "--choice",
            "c",
            "--many",
            "--step",
            "1",
        ])
        .await;

    // Assert
    expect_that!(one.exited(0), eq(true));
    expect_that!(many.exited(0), eq(true));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(
        read["questions"][0]["choices"],
        eq(&json!(["Internal", "Let's Encrypt"]))
    );
    expect_that!(read["questions"][0]["many"], eq(false));
    expect_that!(read["questions"][0]["step"], eq(2));
    expect_that!(read["questions"][1]["choices"], eq(&json!(["a", "b", "c"])));
    expect_that!(read["questions"][1]["many"], eq(true));
    expect_that!(read["questions"][1]["step"], eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_ask_reads_the_details_from_a_file_or_stdin() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("details.txt");
    std::fs::write(&path, "Internal or public\n")?;
    let path = path.to_string_lossy().into_owned();

    // Act
    let from_file = cli
        .run(&["question", "ask", &task, "From a file", "--details-file", &path])
        .await;
    cli.stdin = b"From stdin\nsecond line\n".to_vec();
    let from_stdin = cli
        .run(&["question", "ask", &task, "From stdin", "--details-file", "-"])
        .await;

    // Assert
    expect_that!(from_file.exited(0), eq(true));
    expect_that!(from_stdin.exited(0), eq(true));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["questions"][0]["details"], eq("Internal or public"));
    expect_that!(read["questions"][1]["details"], eq("From stdin\nsecond line"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_request_says_what_it_requested() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let human = cli
        .run(&["question", "request", &task, "Put a staging token in the vault."])
        .await;
    let quiet = cli.run(&["question", "request", &task, "And another", "--quiet"]).await;

    // Assert
    expect_that!(
        human.stdout,
        eq(&format!("Requested {task}.q1: Put a staging token in the vault.\n"))
    );
    expect_that!(human.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(&format!("{task}.q2\n")));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["questions"][0]["kind"], eq("request"));
    expect_that!(read["questions"][0]["choices"], eq(&json!(["Done", "Can't be done"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn many_without_a_choice_is_a_usage_error() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let refused = cli.run(&["question", "ask", &task, "Which?", "--many"]).await;

    // Assert
    expect_that!(refused.exited(2), eq(true));
    expect_that!(refused.stderr, contains_substring("--choice"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_step_the_item_does_not_have_shows_the_servers_refusal() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let unknown = cli
        .run(&["question", "ask", &task, "Which?", "--step", &format!("{task}.9")])
        .await;
    let elsewhere = cli
        .run(&["question", "ask", &task, "Which?", "--step", "zzzzz.1"])
        .await;

    // Assert
    expect_that!(unknown.exited(3), eq(true));
    expect_that!(unknown.stderr, contains_substring("step"));
    expect_that!(elsewhere.exited(2), eq(true));
    expect_that!(elsewhere.stderr, contains_substring("another item"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn asking_on_a_closed_item_says_to_reopen_it_and_an_unknown_item_points_to_the_list() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let fresh = server.read_item(&ada, HEAP, &task).await?;
    server.set_state(&ada, HEAP, &fresh, "done").await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let closed = cli.run(&["question", "ask", &task, "Still there?"]).await;
    let unknown = cli.run(&["question", "ask", "zzzzz", "Anyone?"]).await;

    // Assert
    expect_that!(closed.exited(3), eq(true));
    expect_that!(closed.stderr, contains_substring("upheap item reopen"));
    expect_that!(unknown.exited(4), eq(true));
    expect_that!(unknown.stderr, contains_substring("upheap item list"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_lists_the_questions_open_and_answered_aligned() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    cli.stdin = b"Internal or public\n\tthe tab\n".to_vec();
    cli.run(&[
        "question",
        "ask",
        &task,
        "Which CA?",
        "--choice",
        "Internal",
        "--choice",
        "Let's Encrypt",
        "--step",
        "2",
        "--details-file",
        "-",
    ])
    .await;
    cli.stdin = Vec::new();
    cli.run(&["question", "request", &task, "Put a staging token in the vault."])
        .await;
    cli.run(&[
        "question",
        "ask",
        &task,
        "Which hosts?",
        "--choice",
        "a",
        "--choice",
        "b",
        "--many",
    ])
    .await;
    cli.run(&["question", "ask", &task, "Why?"]).await;
    reply(
        &server,
        &ada,
        &task,
        2,
        json!({ "picked": [0], "comment": "Token is in the vault." }),
    )
    .await?;
    reply(&server, &ada, &task, 3, json!({ "picked": [1, 0] })).await?;
    reply(
        &server,
        &ada,
        &task,
        4,
        json!({ "comment": "Because\r\n\tof the\nrules" }),
    )
    .await?;

    // Act
    let shown = cli.run(&["item", "show", &task]).await;
    let json = cli.run(&["item", "show", &task, "--json"]).await;

    // Assert
    expect_that!(
        shown.stdout,
        ends_with(format!(
            "\nQuestions\n\
             \x20 {task}.q1  Question · open · step {task}.2\n\
             \x20           Which CA?\n\
             \x20             Internal or public\n\
             \x20                 the tab\n\
             \x20           Choices (pick one): Internal · Let's Encrypt\n\
             \x20 {task}.q2  Request · answered\n\
             \x20           Put a staging token in the vault.\n\
             \x20           → Done \"Token is in the vault.\"\n\
             \x20 {task}.q3  Question · answered\n\
             \x20           Which hosts?\n\
             \x20           Choices (pick any): a · b\n\
             \x20           → a, b\n\
             \x20 {task}.q4  Question · answered\n\
             \x20           Why?\n\
             \x20           →\n\
             \x20             Because\n\
             \x20                 of the\n\
             \x20             rules\n"
        ))
    );
    let body: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(body["questions"][1]["reply"]["picked"], eq(&json!([0])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn item_show_has_no_questions_section_for_an_item_without_questions() -> Result<()> {
    // Arrange
    let (server, ada, items, _) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let shown = cli.run(&["item", "show", key(&items[1])?]).await;

    // Assert
    expect_that!(shown.stdout, not(contains_substring("Questions")));
    Ok(())
}

/// Asks a question with `choices` (free text when empty) through the CLI.
async fn ask(cli: &TestCli, task: &str, text: &str, choices: &[&str], many: bool) {
    let mut args = vec!["question", "ask", task, text];
    for choice in choices {
        args.extend(["--choice", choice]);
    }
    if many {
        args.push("--many");
    }
    assert!(cli.run(&args).await.exited(0));
}

#[gtest]
#[tokio::test]
async fn question_reply_picks_by_label_ignoring_case_and_item_show_reads_it_back() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    ask(&cli, &task, "Which CA?", &["Internal", "Let's Encrypt"], false).await;

    // Act
    let replied = cli
        .run(&["question", "reply", &format!("{task}.q1"), "--pick", "let's ENCRYPT"])
        .await;
    let shown = cli.run(&["item", "show", &task]).await;

    // Assert
    expect_that!(replied.stdout, eq(&format!("Replied to {task}.q1: Which CA?\n")));
    expect_that!(replied.stderr, eq(""));
    expect_that!(replied.exited(0), eq(true));
    expect_that!(shown.stdout, contains_substring("Question · answered"));
    expect_that!(shown.stdout, contains_substring("→ Let's Encrypt"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_picks_by_number_and_many_picks_and_json_prints_the_item() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    ask(&cli, &task, "Which CA?", &["Internal", "Let's Encrypt"], false).await;
    ask(&cli, &task, "Which hosts?", &["a", "b", "c"], true).await;

    // Act
    let by_number = cli
        .run(&["question", "reply", &format!("{task}.q1"), "--pick", "2", "--quiet"])
        .await;
    let many = cli
        .run(&[
            "question",
            "reply",
            &format!("{task}.q2"),
            "--pick",
            "c",
            "--pick",
            "1",
            "--pick",
            "C",
            "--json",
        ])
        .await;

    // Assert
    expect_that!(by_number.stdout, eq(""));
    expect_that!(by_number.exited(0), eq(true));
    let body: Value = serde_json::from_str(&many.stdout)?;
    expect_that!(body["key"], eq(&task));
    expect_that!(body["questions"][1]["reply"]["picked"], eq(&json!([0, 2])));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["questions"][0]["reply"]["picked"], eq(&json!([1])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_to_a_request_takes_done_and_a_comment() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    cli.run(&["question", "request", &task, "Put a token in the vault."])
        .await;
    cli.run(&["question", "request", &task, "Open the port."]).await;

    // Act
    let done = cli
        .run(&[
            "question",
            "reply",
            &format!("{task}.q1"),
            "--pick",
            "done",
            "--comment",
            "Put it in.",
        ])
        .await;
    let refused = cli
        .run(&["question", "reply", &format!("{task}.q2"), "--pick", "can't be done"])
        .await;

    // Assert
    expect_that!(done.exited(0), eq(true));
    expect_that!(refused.exited(0), eq(true));
    let shown = cli.run(&["item", "show", &task]).await;
    expect_that!(shown.stdout, contains_substring("→ Done \"Put it in.\""));
    expect_that!(shown.stdout, contains_substring("→ Can't be done"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_reads_the_comment_from_a_file_or_stdin() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let mut cli = cli_in_heap(&server, &ada).await?;
    ask(&cli, &task, "Why?", &[], false).await;
    ask(&cli, &task, "And why not?", &[], false).await;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("comment.txt");
    std::fs::write(&path, "Because of the rules\n")?;
    let path = path.to_string_lossy().into_owned();

    // Act
    let from_file = cli
        .run(&[
            "question",
            "reply",
            &format!("{task}.q1"),
            "--comment-file",
            &path,
            "--quiet",
        ])
        .await;
    cli.stdin = b"From stdin\nsecond line\n".to_vec();
    let from_stdin = cli
        .run(&["question", "reply", &format!("{task}.q2"), "--comment", "-", "--quiet"])
        .await;

    // Assert
    expect_that!(from_file.stdout, eq(""));
    expect_that!(from_file.exited(0), eq(true));
    expect_that!(from_stdin.stdout, eq(""));
    expect_that!(from_stdin.exited(0), eq(true));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["questions"][0]["reply"]["comment"], eq("Because of the rules"));
    expect_that!(read["questions"][1]["reply"]["comment"], eq("From stdin\nsecond line"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_without_a_pick_or_a_comment_is_a_usage_error_and_sends_nothing() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    ask(&cli, &task, "Why?", &[], false).await;

    // Act
    let run = cli.run(&["question", "reply", &format!("{task}.q1")]).await;

    // Assert
    expect_that!(run.exited(2), eq(true));
    expect_that!(run.stderr, contains_substring("--pick"));
    expect_that!(run.stderr, contains_substring("--comment"));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["questions"][0]["state"], eq("open"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_with_an_unknown_pick_lists_the_choices_and_free_text_takes_no_pick() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    ask(&cli, &task, "Which CA?", &["Internal", "Let's Encrypt"], false).await;
    ask(&cli, &task, "Why?", &[], false).await;

    // Act
    let unknown = cli
        .run(&["question", "reply", &format!("{task}.q1"), "--pick", "maybe"])
        .await;
    let free_text = cli
        .run(&["question", "reply", &format!("{task}.q2"), "--pick", "1"])
        .await;

    // Assert
    expect_that!(unknown.exited(3), eq(true));
    expect_that!(
        unknown.stderr,
        contains_substring(format!(
            "{task}.q1 has no choice `maybe`; its choices: Internal, Let's Encrypt"
        ))
    );
    expect_that!(free_text.exited(3), eq(true));
    expect_that!(
        free_text.stderr,
        contains_substring(format!("{task}.q2 takes a free-text reply: give --comment, not --pick"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_to_a_question_the_item_does_not_have_points_to_item_show() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let run = cli
        .run(&["question", "reply", &format!("{task}.q7"), "--comment", "Hi"])
        .await;

    // Assert
    expect_that!(run.exited(4), eq(true));
    expect_that!(run.stderr, contains_substring(format!("no question `{task}.q7`")));
    expect_that!(
        run.stderr,
        contains_substring(format!("`upheap item show {task}` lists its questions"))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_to_an_answered_question_is_refused_with_the_servers_message() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;
    ask(&cli, &task, "Why?", &[], false).await;
    reply(&server, &ada, &task, 1, json!({ "comment": "First" })).await?;

    // Act
    let run = cli
        .run(&["question", "reply", &format!("{task}.q1"), "--comment", "Second"])
        .await;

    // Assert
    expect_that!(run.exited(3), eq(true));
    expect_that!(run.stderr, contains_substring("answered"));
    let read = server.read_item(&ada, HEAP, &task).await?;
    expect_that!(read["questions"][0]["reply"]["comment"], eq("First"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn question_reply_with_a_malformed_name_is_a_usage_error() -> Result<()> {
    // Arrange
    let (server, ada, _, task) = with_task().await?;
    let cli = cli_in_heap(&server, &ada).await?;

    // Act
    let no_q = cli
        .run(&["question", "reply", &format!("{task}.2"), "--comment", "Hi"])
        .await;
    let zero = cli
        .run(&["question", "reply", &format!("{task}.q0"), "--comment", "Hi"])
        .await;

    // Assert
    expect_that!(no_q.exited(2), eq(true));
    expect_that!(no_q.stderr, contains_substring("isn't a question"));
    expect_that!(zero.exited(2), eq(true));
    Ok(())
}
