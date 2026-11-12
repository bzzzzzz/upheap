//! `upheap work run`: the agent loop, against the real API on in-memory SQLite, with a stand-in for the agent tool
//! that plays the agent by running `upheap` the way a run does.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use cli::{Access, Interrupt, TokenStore, Tool, ToolError, ToolReport, ToolStart};
use common::{Agent, Buffer, HEAP, Outcome, StandIn, TestCli, TestServer, TestUser, cli_in_heap, key, wait_until};
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::{Barrier, Notify, Semaphore};

const ONCE: &[&str] = &["work", "run", "--tool", "claude", "--once"];
const LOOP: &[&str] = &["work", "run", "--tool", "claude"];

/// A server with Ada and a heap that prepares every new note.
async fn with_preparing_heap() -> Result<(TestServer, TestUser)> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    change_heap(&server, &ada, json!({ "agents_prepare": true })).await?;
    Ok((server, ada))
}

/// [`with_preparing_heap`], with a note added in it, and the CLI signed in as Ada. Returns the note's key.
async fn with_prepared_note() -> Result<(TestServer, TestUser, TestCli, String)> {
    let (server, ada) = with_preparing_heap().await?;
    let note = add_note(&server, &ada, "Rotate the certs").await?;
    let cli = cli_in_heap(&server, &ada).await?;
    Ok((server, ada, cli, note))
}

async fn change_heap(server: &TestServer, ada: &TestUser, patch: Value) -> Result<()> {
    let heap = server.send(ada, "GET", "/teams/ada/heaps/personal", None).await?;
    server
        .send_with(
            ada,
            "PATCH",
            "/teams/ada/heaps/personal",
            Some(patch),
            &[("if-match", &heap["version"].to_string())],
        )
        .await?;
    Ok(())
}

async fn add_note(server: &TestServer, ada: &TestUser, title: &str) -> Result<String> {
    let note = server.add_item(ada, HEAP, json!({ "title": title })).await?;
    Ok(key(&note)?.to_owned())
}

/// Replies to question `number` of `item` as Ada.
async fn reply(server: &TestServer, ada: &TestUser, item: &str, number: u32, reply: Value) -> Result<Value> {
    server
        .send_with(
            ada,
            "PATCH",
            &format!("/teams/ada/heaps/personal/items/{item}/questions/{number}"),
            Some(json!({ "reply": reply })),
            &[("if-match", "1")],
        )
        .await
}

/// An agent that does nothing and ends normally, with a cost of `cost`.
fn idle_agent(cost: Option<f64>) -> StandIn {
    StandIn::new(move |_| async move {
        Ok(ToolReport {
            cost_usd: cost,
            ..ToolReport::default()
        })
    })
}

/// An agent that writes the item's context and says it reached the goal, for a note.
fn context_writing_agent(agent: &Agent, cost: f64) -> StandIn {
    let agent = agent.clone();
    StandIn::new(move |start: ToolStart| {
        let agent = agent.clone();
        async move {
            let item = item_key_of(&start);
            agent
                .upheap_with_stdin(
                    &start,
                    &["item", "edit", &item, "--context-file", "-", "-q"],
                    "The certs expire.\n",
                )
                .await;
            agent
                .upheap(&start, &["work", "reached", "--result", "Wrote the context"])
                .await;
            Ok(ToolReport {
                cost_usd: Some(cost),
                ..ToolReport::default()
            })
        }
    })
}

/// The key of the item a run works on, read from the brief's item heading line (`k3x9a  Rotate the certs`).
fn item_key_of(start: &ToolStart) -> String {
    let (_, after) = start.brief.split_once("# The item\n\n").unwrap_or_default();
    after.split_whitespace().next().unwrap_or_default().to_owned()
}

fn lines_with<'a>(text: &'a str, needle: &str) -> Vec<&'a str> {
    text.lines().filter(|line| line.contains(needle)).collect()
}

#[gtest]
#[tokio::test]
async fn a_note_gets_a_question_then_its_context_after_the_answer() -> Result<()> {
    // Arrange: the first run asks a question; the second reads the answer and writes the context.
    let (server, ada, cli, note) = with_prepared_note().await?;
    let agent = cli.agent();
    let asking = {
        let (agent, note) = (agent.clone(), note.clone());
        StandIn::new(move |start| {
            let (agent, note) = (agent.clone(), note.clone());
            async move {
                agent.upheap(&start, &["question", "ask", &note, "Which CA?"]).await;
                Ok(ToolReport {
                    cost_usd: Some(0.08),
                    ..ToolReport::default()
                })
            }
        })
    };
    let writing = context_writing_agent(&agent, 0.31);

    // Act
    let asked = cli.run_work(ONCE, &asking).await;
    let waiting = server.read_item(&ada, HEAP, &note).await?;
    reply(&server, &ada, &note, 1, json!({ "comment": "Let's Encrypt" })).await?;
    let reached = cli.run_work(ONCE, &writing).await;
    let item = server.read_item(&ada, HEAP, &note).await?;

    // Assert
    expect_that!(asked.exited(0), eq(true));
    expect_that!(asked.stdout, starts_with("Working in Personal with claude.\n"));
    expect_that!(
        lines_with(&asked.stdout, &note),
        elements_are![matches_regex(format!(
            r"^\d\d:\d\d  {note}  Rotate the certs  note → with context  asked  \d+m\d\ds  \$0\.08$"
        ))]
    );
    expect_that!(waiting["waits_for_you"], eq(true));
    expect_that!(waiting["with_agent"], eq(false));
    expect_that!(reached.exited(0), eq(true));
    expect_that!(
        lines_with(&reached.stdout, &note),
        elements_are![matches_regex(format!(
            r"^\d\d:\d\d  {note}  Rotate the certs  note → with context  reached  \d+m\d\ds  \$0\.31$"
        ))]
    );
    expect_that!(item["stage"], eq("with_context"));
    expect_that!(item["turn"], eq(&Value::Null));
    expect_that!(item["description"], eq("The certs expire."));
    let second_brief = &writing.starts()[0].brief;
    expect_that!(second_brief, contains_substring("Which CA?"));
    expect_that!(second_brief, contains_substring("Let's Encrypt"));
    // The second run is the turn's second: the brief says so.
    expect_that!(
        second_brief,
        ends_with("This turn has had 1 run before this one. Read what is already written and carry on from it.\n")
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_agent_gets_the_instructions_the_heap_the_item_the_goal_and_the_persons_line() -> Result<()> {
    // Arrange: a heap that doesn't prepare, with a context, and a note sent to an agent with a line.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    change_heap(&server, &ada, json!({ "context": "Certs live in the infra repo." })).await?;
    let note = add_note(&server, &ada, "Rotate the certs").await?;
    server
        .send(
            &ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{note}/turn"),
            Some(json!({ "reach": "one", "line": "Use the new CA." })),
        )
        .await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let tool = idle_agent(None);

    // Act
    let outcome = cli.run_work(ONCE, &tool).await;

    // Assert
    expect_that!(outcome.exited(0), eq(true));
    let starts = tool.starts();
    expect_that!(starts.len(), eq(1));
    let start = &starts[0];
    expect_that!(start.access, eq(Access::Prepare));
    expect_that!(start.brief, starts_with("You are working on one item in Upheap"));
    expect_that!(start.brief, contains_substring("Goal: the item has its context."));
    expect_that!(
        start.brief,
        contains_substring("# The heap: Personal (ada/personal)\n\nCerts live in the infra repo.\n\n# The item\n\n")
    );
    expect_that!(start.brief, contains_substring(format!("{note}  Rotate the certs")));
    expect_that!(
        start.brief,
        ends_with("# This run\n\nGoal: with context.\nThe person said: \"Use the new CA.\"\n")
    );
    let var = |name: &str| {
        start
            .env
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    expect_that!(
        var("UPHEAP_RUN").map(|run| uuid::Uuid::parse_str(run).is_ok()),
        some(eq(true))
    );
    expect_that!(
        var("UPHEAP_SERVER").map(|url| url.trim_end_matches('/')),
        some(eq(server.url.as_str()))
    );
    expect_that!(var("UPHEAP_HEAP"), some(eq("ada/personal")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn with_once_and_nothing_to_do_it_says_so_and_never_starts_the_tool() -> Result<()> {
    // Arrange: one heap with nothing, and one whose only item waits for the person.
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let tool = idle_agent(None);
    let empty = {
        let server = TestServer::start().await?;
        let ada = server.register("ada", "Ada").await?;
        cli_in_heap(&server, &ada).await?
    };
    cli.run(&["question", "ask", &note, "Which CA?"]).await;

    // Act
    let none = empty.run_work(ONCE, &tool).await;
    let waiting = cli.run_work(ONCE, &tool).await;

    // Assert
    expect_that!(none.exited(0), eq(true));
    expect_that!(none.stdout, contains_regex(r"(?m)^\d\d:\d\d  Nothing to do\.$"));
    expect_that!(waiting.exited(0), eq(true));
    expect_that!(
        waiting.stdout,
        contains_regex(r"(?m)^\d\d:\d\d  Nothing to do\. 1 item waits for you\.$")
    );
    expect_that!(tool.starts().len(), eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_loop_goes_on_to_the_next_item_then_says_it_is_idle_once_and_stops_on_ctrl_c() -> Result<()> {
    // Arrange: two prepared notes, and an agent that finishes each.
    let (server, ada, cli, first_added) = with_prepared_note().await?;
    let second_added = add_note(&server, &ada, "Tidy the wiki").await?;
    let queue = cli.run(&["work", "next", "--quiet"]).await.stdout.trim().to_owned();
    let tool = context_writing_agent(&cli.agent(), 0.1);
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());

    // Act: the loop runs until the idle line has been shown, then the person presses Ctrl-C.
    let looping = cli.run_work_watched(LOOP, &tool, interrupts, &stdout, &stderr);
    let stopping = async {
        wait_until("the idle line", || stdout.contents().contains("Nothing to do.")).await?;
        // Idle for a few waits: the line is still said once.
        tokio::time::sleep(StdDuration::from_millis(120)).await;
        interrupt.send(Interrupt::CtrlC)?;
        anyhow::Ok(())
    };
    let (outcome, stopped) = tokio::join!(looping, stopping);

    // Assert
    stopped?;
    expect_that!(outcome.exited(0), eq(true));
    let in_order: Vec<&str> = outcome
        .stdout
        .lines()
        .filter(|line| line.contains("reached"))
        .filter_map(|line| line.split("  ").nth(1))
        .collect();
    let other = if queue == first_added {
        &second_added
    } else {
        &first_added
    };
    expect_that!(in_order, elements_are![eq(&queue.as_str()), eq(&other.as_str())]);
    expect_that!(
        lines_with(&outcome.stdout, "Nothing to do."),
        elements_are![matches_regex(
            r"^\d\d:\d\d  Nothing to do\. Asking again in a minute\.$"
        )]
    );
    expect_that!(tool.starts().len(), eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_agent_that_does_nothing_fails_with_the_servers_text_and_a_failed_tool_with_its_own() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let nothing = idle_agent(None);
    let crashing = StandIn::new(|_| async {
        Ok(ToolReport {
            cost_usd: Some(0.4),
            failure: Some("claude ended with error_max_turns.".to_owned()),
            ..ToolReport::default()
        })
    });

    // Act
    let did_nothing = cli.run_work(ONCE, &nothing).await;
    let failed = cli.run_work(ONCE, &crashing).await;

    // Assert
    expect_that!(did_nothing.exited(0), eq(true));
    let lines: Vec<&str> = did_nothing.stdout.lines().skip(1).collect();
    expect_that!(
        lines[0],
        matches_regex(format!(
            r"^\d\d:\d\d  {note}  Rotate the certs  note → with context  failed  \d+m\d\ds$"
        ))
    );
    expect_that!(lines[1], starts_with("       "));
    expect_that!(lines[1].trim().is_empty(), eq(false));
    expect_that!(failed.exited(0), eq(true));
    let lines: Vec<&str> = failed.stdout.lines().skip(1).collect();
    expect_that!(lines[0], ends_with("failed  0m00s  $0.40"));
    expect_that!(lines[1], eq("       claude ended with error_max_turns."));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn three_failures_in_a_row_make_the_turn_stuck_and_the_loop_says_it_waits_for_you() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let tool = StandIn::new(|_| async {
        Ok(ToolReport {
            failure: Some("claude exited with code 1.".to_owned()),
            ..ToolReport::default()
        })
    });
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());

    // Act
    let looping = cli.run_work_watched(LOOP, &tool, interrupts, &stdout, &stderr);
    let stopping = async {
        wait_until("the idle line", || stdout.contents().contains("Nothing to do.")).await?;
        interrupt.send(Interrupt::CtrlC)?;
        anyhow::Ok(())
    };
    let (outcome, stopped) = tokio::join!(looping, stopping);

    // Assert
    stopped?;
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(lines_with(&outcome.stdout, &format!("  {note}  ")).len(), eq(3));
    expect_that!(lines_with(&outcome.stdout, "claude exited with code 1.").len(), eq(3));
    expect_that!(
        lines_with(&outcome.stdout, "Nothing to do."),
        elements_are![contains_substring(
            "Nothing to do. 1 item waits for you. Asking again in a minute."
        )]
    );
    expect_that!(tool.starts().len(), eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn heartbeats_keep_a_long_run_alive_past_the_lapse() -> Result<()> {
    // Arrange: an agent that waits for the test, then writes the context and says reached.
    let (server, ada, mut cli, note) = with_prepared_note().await?;
    cli.heartbeat_every = StdDuration::from_millis(25);
    let (entered, release) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let tool = waiting_agent(&cli.agent(), &entered, &release);

    // Act: the server's clock runs 58 minutes in two steps. After each the CLI signs in again, since the server has
    // let the old access token expire, and the test waits for heartbeats the server has heard. Without them the run
    // would have lapsed at 30 minutes.
    let running = cli.run_work(ONCE, &tool);
    let waiting = async {
        entered.notified().await;
        for _ in 0..2 {
            server.advance(Duration::from_minutes(29));
            cli.tokens.save(&server.url, &server.sign_in_cli(&ada).await?)?;
            // Only the loop reads the tokens meanwhile, once for each heartbeat it starts, one after another: when it
            // starts the second one after the sign-in, the first has been heard.
            let loads = cli.tokens.loads();
            wait_until("two heartbeats after the sign-in", || cli.tokens.loads() >= loads + 2).await?;
        }
        release.notify_one();
        anyhow::Ok(())
    };
    let (outcome, waited) = tokio::join!(running, waiting);

    // Assert
    waited?;
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(
        lines_with(&outcome.stdout, &note),
        elements_are![contains_substring("reached")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn taking_the_turn_back_mid_run_stops_the_tool_and_the_line_says_so() -> Result<()> {
    // Arrange: an agent that never ends on its own.
    let (server, ada, mut cli, note) = with_prepared_note().await?;
    cli.heartbeat_every = StdDuration::from_millis(25);
    let entered = Arc::new(Notify::new());
    let tool = {
        let entered = entered.clone();
        StandIn::new(move |_| {
            let entered = entered.clone();
            async move {
                entered.notify_one();
                std::future::pending::<Result<ToolReport, ToolError>>().await
            }
        })
    };

    // Act
    let running = cli.run_work(ONCE, &tool);
    let taking_back = async {
        entered.notified().await;
        server
            .send(
                &ada,
                "DELETE",
                &format!("/teams/ada/heaps/personal/items/{note}/turn"),
                None,
            )
            .await?;
        anyhow::Ok(())
    };
    let (outcome, taken) = tokio::join!(running, taking_back);

    // Assert
    taken?;
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(
        lines_with(&outcome.stdout, &note),
        elements_are![contains_substring("taken back")]
    );
    expect_that!(tool.stopped(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_tool_still_ending_after_it_reached_keeps_its_cost() -> Result<()> {
    // Arrange: an agent that says reached at once, then takes its time to end, with heartbeats sent meanwhile. The
    // server answers those as it does for a run taken back: the run is over.
    let (_server, _ada, mut cli, note) = with_prepared_note().await?;
    cli.heartbeat_every = StdDuration::from_millis(25);
    let agent = cli.agent();
    let tool = StandIn::new(move |start| {
        let agent = agent.clone();
        async move {
            let item = item_key_of(&start);
            agent
                .upheap_with_stdin(
                    &start,
                    &["item", "edit", &item, "--context-file", "-", "-q"],
                    "Context.\n",
                )
                .await;
            agent.upheap(&start, &["work", "reached", "--result", "Done"]).await;
            tokio::time::sleep(StdDuration::from_millis(300)).await;
            Ok(ToolReport {
                cost_usd: Some(0.31),
                ..ToolReport::default()
            })
        }
    });

    // Act
    let outcome = cli.run_work(ONCE, &tool).await;

    // Assert: the tool was left to end, and what it cost is on the run.
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(tool.stopped(), eq(0));
    expect_that!(
        lines_with(&outcome.stdout, &note),
        elements_are![all![contains_substring("reached"), contains_substring("$0.31")]]
    );
    Ok(())
}

/// An agent that waits for the test to let it end, then writes the context and says reached.
fn waiting_agent(agent: &Agent, entered: &Arc<Notify>, release: &Arc<Notify>) -> StandIn {
    let (agent, entered, release) = (agent.clone(), entered.clone(), release.clone());
    StandIn::new(move |start| {
        let (agent, entered, release) = (agent.clone(), entered.clone(), release.clone());
        async move {
            entered.notify_one();
            release.notified().await;
            let item = item_key_of(&start);
            agent
                .upheap_with_stdin(
                    &start,
                    &["item", "edit", &item, "--context-file", "-", "-q"],
                    "Context.\n",
                )
                .await;
            agent.upheap(&start, &["work", "reached", "--result", "Done"]).await;
            Ok(ToolReport::default())
        }
    })
}

#[gtest]
#[tokio::test]
async fn one_ctrl_c_during_a_run_lets_it_end_and_stops_the_loop_before_the_next_claim() -> Result<()> {
    // Arrange: two prepared notes; the agent waits for the test.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    let (entered, release) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let tool = waiting_agent(&cli.agent(), &entered, &release);
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());

    // Act
    let looping = cli.run_work_watched(LOOP, &tool, interrupts, &stdout, &stderr);
    let pressing = async {
        entered.notified().await;
        interrupt.send(Interrupt::CtrlC)?;
        wait_until("the stopping note", || {
            stderr
                .contents()
                .contains("Stopping after this run. Ctrl-C again stops it now.")
        })
        .await?;
        release.notify_one();
        anyhow::Ok(())
    };
    let (outcome, pressed) = tokio::join!(looping, pressing);
    let next = cli.run(&["work", "next", "--quiet"]).await;

    // Assert
    pressed?;
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(lines_with(&outcome.stdout, "reached").len(), eq(1));
    expect_that!(tool.starts().len(), eq(1));
    // The second note is still in the queue, not started.
    expect_that!(next.stdout.trim().is_empty(), eq(false));
    expect_that!(outcome.stderr, contains_substring("Stopping after this run."));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_second_ctrl_c_stops_the_tool_and_the_run_counts_as_failed() -> Result<()> {
    // Arrange: an agent that never ends on its own.
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let entered = Arc::new(Notify::new());
    let tool = {
        let entered = entered.clone();
        StandIn::new(move |_| {
            let entered = entered.clone();
            async move {
                entered.notify_one();
                std::future::pending::<Result<ToolReport, ToolError>>().await
            }
        })
    };
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());

    // Act
    let looping = cli.run_work_watched(LOOP, &tool, interrupts, &stdout, &stderr);
    let pressing = async {
        entered.notified().await;
        interrupt.send(Interrupt::CtrlC)?;
        wait_until("the stopping note", || {
            stderr.contents().contains("Stopping after this run.")
        })
        .await?;
        interrupt.send(Interrupt::CtrlC)?;
        anyhow::Ok(())
    };
    let (outcome, pressed) = tokio::join!(looping, pressing);

    // Assert
    pressed?;
    expect_that!(outcome.exited(0), eq(true));
    let lines: Vec<&str> = outcome.stdout.lines().skip(1).collect();
    expect_that!(lines[0], contains_substring(format!("  {note}  ")));
    expect_that!(lines[0], contains_substring("  failed  "));
    expect_that!(lines[1], eq("       Stopped by the person."));
    expect_that!(tool.stopped(), eq(1));
    Ok(())
}

/// An agent that waits until the test has let `gate` give it a permit, then writes the context and says reached.
fn gated_agent(agent: &Agent, gate: &Arc<Semaphore>) -> StandIn {
    let (agent, gate) = (agent.clone(), gate.clone());
    StandIn::new(move |start| {
        let (agent, gate) = (agent.clone(), gate.clone());
        async move {
            if let Ok(permit) = gate.acquire().await {
                permit.forget();
            }
            let item = item_key_of(&start);
            agent
                .upheap_with_stdin(
                    &start,
                    &["item", "edit", &item, "--context-file", "-", "-q"],
                    "Context.\n",
                )
                .await;
            agent.upheap(&start, &["work", "reached", "--result", "Done"]).await;
            Ok(ToolReport::default())
        }
    })
}

/// An agent that never ends on its own.
fn endless_agent() -> StandIn {
    StandIn::new(|_| std::future::pending::<Result<ToolReport, ToolError>>())
}

/// `--workers <n> --root <a new directory>`, with the directory kept alive by the returned guard.
fn workers_in_a_root(n: u8) -> Result<(Vec<String>, tempfile::TempDir)> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("runs").to_string_lossy().into_owned();
    let args = ["--workers".to_owned(), n.to_string(), "--root".to_owned(), root].to_vec();
    Ok((args, parent))
}

fn with_args(base: &[&str], extra: &[String]) -> Vec<String> {
    base.iter()
        .map(|arg| (*arg).to_owned())
        .chain(extra.iter().cloned())
        .collect()
}

/// Runs `args` as `work run`, failing the test instead of hanging when it takes more than 20 seconds.
async fn run_work_bounded(cli: &TestCli, args: &[String], tool: &StandIn) -> Result<Outcome> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(tokio::time::timeout(StdDuration::from_secs(20), cli.run_work(&args, tool)).await?)
}

#[gtest]
#[tokio::test]
async fn two_workers_work_two_items_at_once() -> Result<()> {
    // Arrange: two prepared notes, and an agent that finishes only once both runs are in.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    let barrier = Arc::new(Barrier::new(2));
    let tool = {
        let (agent, barrier) = (cli.agent(), barrier.clone());
        StandIn::new(move |start| {
            let (agent, barrier) = (agent.clone(), barrier.clone());
            async move {
                barrier.wait().await;
                let item = item_key_of(&start);
                agent
                    .upheap_with_stdin(
                        &start,
                        &["item", "edit", &item, "--context-file", "-", "-q"],
                        "Context.\n",
                    )
                    .await;
                agent.upheap(&start, &["work", "reached", "--result", "Done"]).await;
                Ok(ToolReport::default())
            }
        })
    };
    let (args, _root) = workers_in_a_root(2)?;

    // Act: one worker alone would wait at the barrier for ever, so the run is bounded.
    let outcome = run_work_bounded(&cli, &with_args(ONCE, &args), &tool).await?;

    // Assert
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(
        outcome.stdout,
        starts_with("Working in Personal with claude, 2 runs at a time.\n")
    );
    let keys: std::collections::BTreeSet<String> = tool.starts().iter().map(item_key_of).collect();
    expect_that!(keys.len(), eq(2));
    expect_that!(
        lines_with(&outcome.stdout, "reached"),
        elements_are![
            matches_regex(r"^\d\d:\d\d  \w+  .*  reached  \d+m\d\ds$"),
            matches_regex(r"^\d\d:\d\d  \w+  .*  reached  \d+m\d\ds$")
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn one_ctrl_c_with_two_runs_lets_both_end_and_claims_nothing_new() -> Result<()> {
    // Arrange: three prepared notes, two workers.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    add_note(&server, &ada, "Water the plants").await?;
    let gate = Arc::new(Semaphore::new(0));
    let tool = gated_agent(&cli.agent(), &gate);
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());
    let (args, _root) = workers_in_a_root(2)?;
    let args = with_args(LOOP, &args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    // Act
    let looping = tokio::time::timeout(
        StdDuration::from_secs(20),
        cli.run_work_watched(&args, &tool, interrupts, &stdout, &stderr),
    );
    let pressing = async {
        wait_until("both runs", || tool.starts().len() == 2).await?;
        interrupt.send(Interrupt::CtrlC)?;
        wait_until("the stopping note", || {
            stderr
                .contents()
                .contains("Stopping after the current runs. Ctrl-C again stops them now.")
        })
        .await?;
        gate.add_permits(2);
        anyhow::Ok(())
    };
    let (outcome, pressed) = tokio::join!(looping, pressing);
    let outcome = outcome?;
    let next = cli.run(&["work", "next", "--quiet"]).await;

    // Assert
    pressed?;
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(lines_with(&outcome.stdout, "reached").len(), eq(2));
    expect_that!(tool.starts().len(), eq(2));
    expect_that!(next.stdout.trim().is_empty(), eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_second_ctrl_c_stops_every_run() -> Result<()> {
    // Arrange: two notes, two workers, agents that never end.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    let tool = endless_agent();
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());
    let (args, _root) = workers_in_a_root(2)?;
    let args = with_args(LOOP, &args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    // Act
    let looping = tokio::time::timeout(
        StdDuration::from_secs(20),
        cli.run_work_watched(&args, &tool, interrupts, &stdout, &stderr),
    );
    let pressing = async {
        wait_until("both runs", || tool.starts().len() == 2).await?;
        interrupt.send(Interrupt::CtrlC)?;
        wait_until("the stopping note", || {
            stderr.contents().contains("Stopping after the current runs.")
        })
        .await?;
        interrupt.send(Interrupt::CtrlC)?;
        anyhow::Ok(())
    };
    let (outcome, pressed) = tokio::join!(looping, pressing);
    let outcome = outcome?;

    // Assert
    pressed?;
    expect_that!(outcome.exited(0), eq(true));
    let lines: Vec<&str> = outcome.stdout.lines().skip(1).collect();
    expect_that!(lines.len(), eq(4));
    for pair in lines.chunks(2) {
        expect_that!(pair[0], contains_substring("  failed  "));
        expect_that!(pair[1], eq("       Stopped by the person."));
    }
    expect_that!(tool.stopped(), eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn several_workers_without_a_root_are_refused_before_any_request() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, _note) = with_prepared_note().await?;
    let tool = idle_agent(None);

    // Act
    let outcome = cli.run_work(&[ONCE, &["--workers", "2"]].concat(), &tool).await;
    let next = cli.run(&["work", "next", "--quiet"]).await;

    // Assert
    expect_that!(outcome.exited(2), eq(true));
    expect_that!(outcome.stderr, contains_substring("add --root"));
    expect_that!(tool.starts().len(), eq(0));
    expect_that!(next.stdout.trim().is_empty(), eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn workers_out_of_range_are_refused() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, _note) = with_prepared_note().await?;
    let tool = idle_agent(None);

    // Act
    let none = cli.run_work(&[ONCE, &["--workers", "0"]].concat(), &tool).await;
    let nine = cli.run_work(&[ONCE, &["--workers", "9"]].concat(), &tool).await;

    // Assert
    expect_that!(none.exited(2), eq(true));
    expect_that!(nine.exited(2), eq(true));
    expect_that!(tool.starts().len(), eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn more_workers_than_items_say_nothing_to_do_at_most_once() -> Result<()> {
    // Arrange: one note, three workers.
    let (_server, _ada, cli, _note) = with_prepared_note().await?;
    let tool = context_writing_agent(&cli.agent(), 0.1);
    let (args, _root) = workers_in_a_root(3)?;

    // Act
    let outcome = run_work_bounded(&cli, &with_args(ONCE, &args), &tool).await?;

    // Assert
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(tool.starts().len(), eq(1));
    expect_that!(lines_with(&outcome.stdout, "reached").len(), eq(1));
    expect_that!(lines_with(&outcome.stdout, "Nothing to do").len(), le(1));
    Ok(())
}

/// Ada's defined task with two steps, in a heap that doesn't prepare, sent to an agent all the way. Returns the key.
async fn defined_task_with_go_ahead(server: &TestServer, ada: &TestUser) -> Result<String> {
    let task = add_note(server, ada, "Paint the fence").await?;
    server
        .edit_item(
            ada,
            HEAP,
            &task,
            json!({ "kind": "task", "description": "Done when: the fence is painted." }),
        )
        .await?;
    for title in ["Buy paint", "Paint it"] {
        server
            .send(
                ada,
                "POST",
                &format!("/teams/ada/heaps/personal/items/{task}/steps"),
                Some(json!({ "title": title })),
            )
            .await?;
    }
    server
        .edit_item(ada, HEAP, &task, json!({ "stage": "defined" }))
        .await?;
    server
        .send(
            ada,
            "POST",
            &format!("/teams/ada/heaps/personal/items/{task}/turn"),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    Ok(task)
}

#[gtest]
#[tokio::test]
async fn doing_a_task_ends_with_a_request_to_review_and_the_next_run_reads_the_reply() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let task = defined_task_with_go_ahead(&server, &ada).await?;
    let cli = cli_in_heap(&server, &ada).await?;
    let agent = cli.agent();
    let working = {
        let (agent, task) = (agent.clone(), task.clone());
        StandIn::new(move |start| {
            let (agent, task) = (agent.clone(), task.clone());
            async move {
                agent.upheap(&start, &["item", "start", &task]).await;
                agent.upheap(&start, &["step", "tick", &format!("{task}.1")]).await;
                agent.upheap(&start, &["step", "tick", &format!("{task}.2")]).await;
                agent
                    .upheap_with_stdin(
                        &start,
                        &[
                            "question",
                            "request",
                            &task,
                            "Review https://example.com/pr/7 and mark the task done",
                            "--details-file",
                            "-",
                        ],
                        "Painted the fence; checked with a second coat.\n",
                    )
                    .await;
                Ok(ToolReport {
                    cost_usd: Some(1.25),
                    ..ToolReport::default()
                })
            }
        })
    };
    let again = idle_agent(None);

    // Act
    let first = cli.run_work(ONCE, &working).await;
    let waiting = server.read_item(&ada, HEAP, &task).await?;
    reply(
        &server,
        &ada,
        &task,
        1,
        json!({ "picked": [1], "comment": "The corner is missed." }),
    )
    .await?;
    let second = cli.run_work(ONCE, &again).await;
    server.edit_item(&ada, HEAP, &task, json!({ "state": "done" })).await?;
    let third = cli.run_work(ONCE, &again).await;

    // Assert
    expect_that!(first.exited(0), eq(true));
    let start = &working.starts()[0];
    expect_that!(start.access, eq(Access::Do));
    expect_that!(
        start.brief,
        contains_substring("Goal: the task is done, as far as you can take it.")
    );
    expect_that!(start.brief, ends_with("Goal: done.\n"));
    expect_that!(
        lines_with(&first.stdout, &task),
        elements_are![matches_regex(format!(
            r"^\d\d:\d\d  {task}  Paint the fence  task → done  asked  \d+m\d\ds  \$1\.25$"
        ))]
    );
    expect_that!(waiting["state"], eq("in_progress"));
    expect_that!(waiting["step_progress"], eq(&json!({ "ticked": 2, "total": 2 })));
    expect_that!(waiting["waits_for_you"], eq(true));
    expect_that!(second.exited(0), eq(true));
    let second_brief = &again.starts()[0].brief;
    expect_that!(
        second_brief,
        contains_substring("Review https://example.com/pr/7 and mark the task done")
    );
    expect_that!(second_brief, contains_substring("Can't be done"));
    expect_that!(second_brief, contains_substring("The corner is missed."));
    expect_that!(third.exited(0), eq(true));
    expect_that!(third.stdout, contains_regex(r"(?m)^\d\d:\d\d  Nothing to do\.$"));
    expect_that!(again.starts().len(), eq(1));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn with_a_root_each_run_gets_a_directory_of_its_own_under_it_named_by_its_item() -> Result<()> {
    // Arrange: two notes to prepare, and a root that isn't there yet.
    let (server, ada, cli, first) = with_prepared_note().await?;
    let second = add_note(&server, &ada, "Order the certificate").await?;
    let tool = idle_agent(None);
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("runs");
    let root_arg = root.to_string_lossy().into_owned();
    let once_in_root = [ONCE, &["--root", &root_arg]].concat();

    // Act
    let one = cli.run_work(&once_in_root, &tool).await;
    let two = cli.run_work(&once_in_root, &tool).await;
    let without = cli.run_work(ONCE, &tool).await;

    // Assert: the loop makes the root, and hands the tool a directory to make in it.
    expect_that!(one.exited(0), eq(true));
    expect_that!(two.exited(0), eq(true));
    expect_that!(without.exited(0), eq(true));
    expect_that!(root.is_dir(), eq(true));
    let starts = tool.starts();
    let dirs: Vec<_> = starts.iter().map(|start| start.dir.clone()).collect();
    assert_that!(dirs.len(), eq(3));
    let (Some(a), Some(b)) = (&dirs[0], &dirs[1]) else {
        anyhow::bail!("the runs with a root have no directory: {dirs:?}");
    };
    expect_that!(a.parent(), some(eq(root.as_path())));
    expect_that!(b.parent(), some(eq(root.as_path())));
    expect_that!(a, not(eq(b)));
    let names = [a, b].map(|dir| dir.file_name().unwrap_or_default().to_string_lossy().into_owned());
    expect_that!(names, each(matches_regex(r"[0-9a-z]+-[0-9a-f-]{36}")));
    // Each is named by the key of the item it worked, whichever of the two the queue gave it.
    let keys = [format!("{first}-"), format!("{second}-")];
    expect_that!(
        names.iter().all(|name| keys.iter().any(|key| name.starts_with(key))),
        eq(true)
    );
    expect_that!(dirs[2], none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_root_that_cannot_be_made_is_exit_6_and_nothing_is_claimed() -> Result<()> {
    // Arrange: the root would be inside a file.
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let parent = tempfile::tempdir()?;
    let file = parent.path().join("a-file");
    std::fs::write(&file, "")?;
    let root = file.join("runs").to_string_lossy().into_owned();
    let tool = idle_agent(None);

    // Act
    let outcome = cli.run_work(&[ONCE, &["--root", &root]].concat(), &tool).await;
    let next = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(outcome.exited(6), eq(true));
    expect_that!(
        outcome.stderr,
        starts_with(format!("error: couldn't make a directory for the run at {root}").as_str())
    );
    expect_that!(
        outcome.stderr,
        contains_substring("hint: check that `--root` names a directory you can write to")
    );
    expect_that!(tool.starts().len(), eq(0));
    expect_that!(next.stdout, not(contains_substring("started")));
    expect_that!(next.stdout, contains_substring(note.as_str()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn with_instructions_every_run_is_told_them_before_the_heap() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, _note) = with_prepared_note().await?;
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("how-i-work.md");
    std::fs::write(&file, "Use the implement skill for code changes.\n")?;
    let file_arg = file.to_string_lossy().into_owned();
    let tool = idle_agent(None);

    // Act
    let with = cli
        .run_work(&[ONCE, &["--instructions", &file_arg]].concat(), &tool)
        .await;
    let without = cli.run_work(ONCE, &tool).await;

    // Assert
    expect_that!(with.exited(0), eq(true));
    expect_that!(without.exited(0), eq(true));
    let starts = tool.starts();
    assert_that!(starts.len(), eq(2));
    let told = starts[0].brief.find("Use the implement skill for code changes.");
    expect_that!(told, some(anything()));
    expect_that!(told < starts[0].brief.find("# The heap:"), eq(true));
    expect_that!(starts[1].brief, not(contains_substring("The runner's instructions")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn instructions_that_cannot_be_read_are_exit_6_and_nothing_is_claimed() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, note) = with_prepared_note().await?;
    let dir = tempfile::tempdir()?;
    let missing = dir.path().join("not-there.md").to_string_lossy().into_owned();
    let tool = idle_agent(None);

    // Act
    let outcome = cli
        .run_work(&[ONCE, &["--instructions", &missing]].concat(), &tool)
        .await;
    let next = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(outcome.exited(6), eq(true));
    expect_that!(
        outcome.stderr,
        starts_with(format!("error: couldn't read the instructions at {missing}").as_str())
    );
    expect_that!(
        outcome.stderr,
        contains_substring("hint: check that `--instructions` names a text file you can read")
    );
    expect_that!(tool.starts().len(), eq(0));
    expect_that!(next.stdout, not(contains_substring("started")));
    expect_that!(next.stdout, contains_substring(note.as_str()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_tool_that_is_not_installed_is_exit_6_and_nothing_is_claimed() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, note) = with_prepared_note().await?;

    // Act
    let outcome = cli.run_work(ONCE, &StandIn::not_installed()).await;
    let next = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(outcome.exited(6), eq(true));
    expect_that!(outcome.stdout, eq(""));
    expect_that!(
        outcome.stderr,
        eq("error: claude isn't installed, or isn't on PATH\n\
            hint: install Claude Code, or check that `claude --version` works in this shell\n")
    );
    expect_that!(next.stdout, contains_substring(note.as_str()));
    expect_that!(next.stdout, not(contains_substring("started")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn inside_a_run_it_is_refused_before_any_request() -> Result<()> {
    // Arrange
    let (_server, _ada, mut cli, note) = with_prepared_note().await?;
    let tool = idle_agent(None);
    cli.env.run = Some("018f0000-0000-7000-8000-000000000000".to_owned());

    // Act
    let outcome = cli.run_work(ONCE, &tool).await;
    cli.env.run = None;
    let next = cli.run(&["work", "next"]).await;

    // Assert
    expect_that!(outcome.exited(2), eq(true));
    expect_that!(
        outcome.stderr,
        contains_substring("work run starts runs; unset UPHEAP_RUN first")
    );
    expect_that!(tool.starts().len(), eq(0));
    expect_that!(next.stdout, contains_substring(note.as_str()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn without_a_heap_and_a_default_it_is_a_usage_error() -> Result<()> {
    // Arrange
    let (_server, _ada, mut cli, _note) = with_prepared_note().await?;
    cli.env.heap = None;

    // Act
    let outcome = cli.run_work(ONCE, &idle_agent(None)).await;

    // Assert
    expect_that!(outcome.exited(2), eq(true));
    expect_that!(outcome.stderr, contains_substring("heap"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_tool_flag_is_required_and_knows_claude_only() -> Result<()> {
    // Arrange
    let (_server, _ada, cli, _note) = with_prepared_note().await?;

    // Act
    let missing = cli.run(&["work", "run", "--once"]).await;
    let other = cli.run(&["work", "run", "--tool", "gpt", "--once"]).await;

    // Assert
    expect_that!(missing.exited(2), eq(true));
    expect_that!(missing.stderr, contains_substring("--tool <TOOL>"));
    expect_that!(other.exited(2), eq(true));
    expect_that!(other.stderr, contains_substring("[possible values: claude]"));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn quiet_prints_the_key_of_each_run_and_json_the_run_as_the_server_has_it() -> Result<()> {
    // Arrange: two notes in the queue, one for each run.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    let first = cli.run(&["work", "next", "--quiet"]).await.stdout.trim().to_owned();
    let tool = context_writing_agent(&cli.agent(), 0.05);

    // Act
    let quiet = cli
        .run_work(&["work", "run", "--tool", "claude", "--once", "--quiet"], &tool)
        .await;
    let json = cli
        .run_work(&["work", "run", "--tool", "claude", "--once", "--json"], &tool)
        .await;

    // Assert
    expect_that!(quiet.exited(0), eq(true));
    expect_that!(quiet.stdout, eq(&format!("{first}\n")));
    expect_that!(quiet.stderr, eq(""));
    expect_that!(json.exited(0), eq(true));
    expect_that!(json.stdout.lines().count(), eq(1));
    let run: Value = serde_json::from_str(&json.stdout)?;
    expect_that!(run["end"], eq("reached"));
    expect_that!(run["cost_usd"], eq(0.05));
    expect_that!(run["item"]["key"], not(eq(&json!(first))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_server_that_cannot_be_reached_is_exit_5_with_once() -> Result<()> {
    // Arrange: signed in to a server, then pointed at a port nobody listens on.
    let (server, _ada, mut cli, _note) = with_prepared_note().await?;
    let dead = "http://127.0.0.1:9";
    let tokens = cli.stored(&server)?.context("a sign-in")?;
    cli.tokens.save(dead, &tokens)?;
    cli.env.server = Some(dead.to_owned());
    let tool = idle_agent(None);

    // Act
    let outcome: Outcome = cli
        .run_work(&["work", "run", "--tool", "claude", "--once", "--heap", HEAP], &tool)
        .await;

    // Assert
    expect_that!(outcome.exited(5), eq(true));
    expect_that!(tool.starts().len(), eq(0));
    Ok(())
}

/// An agent whose first run is gated and says it is live, and whose second can't start once the first is live: the
/// error that ends the loop while another run is going on.
fn one_run_live_then_one_that_cannot_start(agent: &Agent, gate: &Arc<Semaphore>) -> StandIn {
    let (agent, gate) = (agent.clone(), gate.clone());
    let (live, count) = (Arc::new(Notify::new()), Arc::new(AtomicUsize::new(0)));
    StandIn::new(move |start| {
        let (agent, gate, live) = (agent.clone(), gate.clone(), live.clone());
        let first = count.fetch_add(1, Ordering::SeqCst) == 0;
        async move {
            if first {
                live.notify_one();
                if let Ok(permit) = gate.acquire().await {
                    permit.forget();
                }
                let item = item_key_of(&start);
                agent
                    .upheap_with_stdin(
                        &start,
                        &["item", "edit", &item, "--context-file", "-", "-q"],
                        "Context.\n",
                    )
                    .await;
                agent.upheap(&start, &["work", "reached", "--result", "Done"]).await;
                Ok(ToolReport::default())
            } else {
                live.notified().await;
                Err(ToolError::WouldNotStart {
                    tool: Tool::Claude,
                    source: std::io::Error::other("no such luck"),
                })
            }
        }
    })
}

#[gtest]
#[tokio::test]
async fn a_worker_that_fails_lets_the_other_run_end_claims_nothing_new_and_fails_the_command() -> Result<()> {
    // Arrange: three notes, two workers; one run goes live, the other can't start.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    add_note(&server, &ada, "Water the plants").await?;
    let gate = Arc::new(Semaphore::new(0));
    let tool = one_run_live_then_one_that_cannot_start(&cli.agent(), &gate);
    let (_interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());
    let (args, _root) = workers_in_a_root(2)?;
    let args = with_args(LOOP, &args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    // Act: the live run is released only once the loop has said it is stopping.
    let looping = tokio::time::timeout(
        StdDuration::from_secs(20),
        cli.run_work_watched(&args, &tool, interrupts, &stdout, &stderr),
    );
    let releasing = async {
        wait_until("the stopping note", || {
            stderr.contents().contains("Stopping after the current runs:")
        })
        .await?;
        gate.add_permits(1);
        anyhow::Ok(())
    };
    let (outcome, released) = tokio::join!(looping, releasing);
    let outcome = outcome?;
    let next = cli.run(&["work", "next", "--quiet"]).await;

    // Assert
    released?;
    expect_that!(outcome.exited(6), eq(true));
    expect_that!(outcome.stderr, contains_substring("Stopping after the current runs: "));
    expect_that!(outcome.stderr, contains_substring("Ctrl-C twice stops them now."));
    expect_that!(lines_with(&outcome.stdout, "reached").len(), eq(1));
    expect_that!(lines_with(&outcome.stdout, "failed").len(), eq(1));
    expect_that!(tool.starts().len(), eq(2));
    expect_that!(next.stdout.trim().is_empty(), eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn after_a_worker_fails_the_first_ctrl_c_still_lets_the_live_run_end() -> Result<()> {
    // Arrange: as above.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    let gate = Arc::new(Semaphore::new(0));
    let tool = one_run_live_then_one_that_cannot_start(&cli.agent(), &gate);
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());
    let (args, _root) = workers_in_a_root(2)?;
    let args = with_args(LOOP, &args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    // Act: one Ctrl-C after the failure; the run is released only when the loop has taken the press as the gentle one.
    let looping = tokio::time::timeout(
        StdDuration::from_secs(20),
        cli.run_work_watched(&args, &tool, interrupts, &stdout, &stderr),
    );
    let pressing = async {
        wait_until("the failure note", || {
            stderr.contents().contains("Stopping after the current runs:")
        })
        .await?;
        interrupt.send(Interrupt::CtrlC)?;
        wait_until("the gentle note", || {
            stderr.contents().contains("Ctrl-C again stops them now.")
        })
        .await?;
        gate.add_permits(1);
        anyhow::Ok(())
    };
    let (outcome, pressed) = tokio::join!(looping, pressing);
    let outcome = outcome?;

    // Assert
    pressed?;
    expect_that!(outcome.exited(6), eq(true));
    expect_that!(lines_with(&outcome.stdout, "reached").len(), eq(1));
    expect_that!(outcome.stdout, not(contains_substring("Stopped by the person.")));
    expect_that!(tool.stopped(), eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_worker_claims_again_while_another_run_goes_on() -> Result<()> {
    // Arrange: three notes, two workers; the first run is gated, the others are fast.
    let (server, ada, cli, _note) = with_prepared_note().await?;
    add_note(&server, &ada, "Tidy the wiki").await?;
    add_note(&server, &ada, "Water the plants").await?;
    let gate = Arc::new(Semaphore::new(0));
    let tool = {
        let (agent, gate, count) = (cli.agent(), gate.clone(), Arc::new(AtomicUsize::new(0)));
        StandIn::new(move |start| {
            let (agent, gate) = (agent.clone(), gate.clone());
            let gated = count.fetch_add(1, Ordering::SeqCst) == 0;
            async move {
                if gated && let Ok(permit) = gate.acquire().await {
                    permit.forget();
                }
                let item = item_key_of(&start);
                agent
                    .upheap_with_stdin(
                        &start,
                        &["item", "edit", &item, "--context-file", "-", "-q"],
                        "Context.\n",
                    )
                    .await;
                agent.upheap(&start, &["work", "reached", "--result", "Done"]).await;
                Ok(ToolReport::default())
            }
        })
    };
    let (interrupt, interrupts) = unbounded_channel();
    let (stdout, stderr) = (Buffer::default(), Buffer::default());
    let (args, _root) = workers_in_a_root(2)?;
    let args = with_args(LOOP, &args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    // Act: both fast runs end while the gated one is still live; then the person stops after runs and it is released.
    let looping = tokio::time::timeout(
        StdDuration::from_secs(20),
        cli.run_work_watched(&args, &tool, interrupts, &stdout, &stderr),
    );
    let pressing = async {
        wait_until("two runs ended while one is live", || {
            lines_with(&stdout.contents(), "reached").len() == 2
        })
        .await?;
        interrupt.send(Interrupt::CtrlC)?;
        wait_until("the stopping note", || stderr.contents().contains("Stopping after")).await?;
        gate.add_permits(1);
        anyhow::Ok(())
    };
    let (outcome, pressed) = tokio::join!(looping, pressing);
    let outcome = outcome?;

    // Assert
    pressed?;
    expect_that!(outcome.exited(0), eq(true));
    expect_that!(lines_with(&outcome.stdout, "reached").len(), eq(3));
    expect_that!(tool.starts().len(), eq(3));
    Ok(())
}
