//! The golden path of "Work with agents", end to end: one person, Ada, on her own server, through the real CLI and
//! API, with a stand-in for the agent tool. Each part has its own tests; this one proves they still fit together.

mod common;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use cli::{ToolReport, ToolStart};
use common::{Agent, Outcome, StandIn, TestCli, TestServer, TestUser, change_heap, lines_with};
use googletest::prelude::*;
use serde_json::{Value, json};

const HEAP: &str = "ada/website";
const ONCE: &[&str] = &["work", "run", "--tool", "claude", "--once"];

/// What the agent's `upheap` calls returned, to check afterwards that none was refused.
type Outcomes = Arc<Mutex<Vec<Outcome>>>;

/// An agent that runs each of `commands` as `upheap <args>`, with the stdin given beside it, then ends the run with
/// `cost` dollars.
fn plays(agent: &Agent, outcomes: &Outcomes, cost: f64, commands: Vec<(Vec<String>, &'static str)>) -> StandIn {
    let (agent, outcomes) = (agent.clone(), outcomes.clone());
    StandIn::new(move |start: ToolStart| {
        let (agent, outcomes, commands) = (agent.clone(), outcomes.clone(), commands.clone());
        async move {
            for (args, stdin) in &commands {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let outcome = agent.upheap_with_stdin(&start, &args, stdin).await;
                locked(&outcomes).push(outcome);
            }
            Ok(ToolReport {
                cost_usd: Some(cost),
                ..ToolReport::default()
            })
        }
    })
}

/// The outcomes so far; a panic elsewhere in the test doesn't hide them.
fn locked(outcomes: &Outcomes) -> std::sync::MutexGuard<'_, Vec<Outcome>> {
    outcomes.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The arguments of one agent command.
fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

/// Asserts that every `upheap` call the agent made since the last check exited 0, and forgets them.
fn all_succeeded(outcomes: &Outcomes) -> Result<()> {
    let made = std::mem::take(&mut *locked(outcomes));
    for outcome in &made {
        expect_that!(
            outcome.exited(0),
            eq(true),
            "an agent command failed: {}",
            outcome.stderr
        );
    }
    Ok(())
}

/// Ada sends `item` to an agent, with `reach` and, when given, `goal`. Returns the item.
async fn send(server: &TestServer, ada: &TestUser, item: &str, reach: &str, goal: Option<&str>) -> Result<Value> {
    let mut body = json!({ "reach": reach });
    if let Some(goal) = goal {
        body["goal"] = json!(goal);
    }
    server
        .send(
            ada,
            "POST",
            &format!("/teams/ada/heaps/website/items/{item}/turn"),
            Some(body),
        )
        .await
}

/// Ada replies to `question` (`k3x9a.q1`) through the CLI.
async fn reply(cli: &TestCli, question: &str, extra: &[&str]) -> Result<()> {
    let mut command = vec!["question", "reply", question];
    command.extend_from_slice(extra);
    let replied = cli.run(&command).await;
    expect_that!(replied.exited(0), eq(true), "{}", replied.stderr);
    Ok(())
}

/// Runs the loop once and checks it ended well.
async fn run_once(cli: &TestCli, tool: &StandIn) -> Outcome {
    let ran = cli.run_work(ONCE, tool).await;
    expect_that!(ran.exited(0), eq(true), "{}", ran.stderr);
    ran
}

#[gtest]
#[tokio::test]
async fn the_golden_path_from_an_empty_heap_to_the_second_task_done() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let agent = cli.agent();
    let outcomes = Outcomes::default();

    // 1. Ada makes a heap for the project, writes its context and sets it so agents prepare what she adds.
    let added = cli.run(&["heap", "add", "Website"]).await;
    expect_that!(added.exited(0), eq(true), "{}", added.stderr);
    expect_that!(cli.run(&["heap", "use", "website"]).await.exited(0), eq(true));
    let mut cli = cli;
    cli.stdin = b"The site is in github.com/ada/website. Docs are in docs/. Review on GitHub.\n".to_vec();
    let written = cli.run(&["heap", "edit", "--context-file", "-"]).await;
    expect_that!(written.exited(0), eq(true), "{}", written.stderr);
    change_heap(&server, &ada, HEAP, json!({ "agents_prepare": true })).await?;
    let shown = cli.run(&["heap", "show"]).await;
    expect_that!(shown.stdout, contains_substring("agents prepare new notes: on"));
    expect_that!(shown.stdout, contains_substring("github.com/ada/website"));

    // 3 (before there is anything). With nothing for an agent, the loop says so and never starts the tool.
    let nothing_yet = plays(&agent, &outcomes, 9.99, vec![]);
    let idle = run_once(&cli, &nothing_yet).await;
    expect_that!(idle.stdout, contains_regex(r"(?m)^\d\d:\d\d  Nothing to do\.$"));
    expect_that!(nothing_yet.starts().len(), eq(0));

    // 2. An idea comes up: Ada adds a note with a need.
    let added = cli
        .run(&["item", "add", "Add a contact page", "--need", "expected", "-q"])
        .await;
    expect_that!(added.exited(0), eq(true), "{}", added.stderr);
    let note = added.stdout.trim().to_owned();
    let listed = cli.run(&["item", "list"]).await;
    expect_that!(lines_with(&listed.stdout, &note).len(), eq(1));
    let read = server.read_item(&ada, HEAP, &note).await?;
    expect_that!(read["kind"], eq("note"));
    expect_that!(read["turn"]["source"], eq("prepare"));

    // 3. `work next` names the note, and the first run claims it and starts the tool exactly once.
    let next = cli.run(&["work", "next", "-q"]).await;
    expect_that!(next.stdout.trim(), eq(note.as_str()));

    // 4. The agent writes the note's context and asks a question; the note waits for Ada.
    let asking = plays(
        &agent,
        &outcomes,
        0.11,
        vec![
            (
                args(&["item", "edit", &note, "--context-file", "-", "-q"]),
                "A page with the address and a form.\n",
            ),
            (args(&["question", "ask", &note, "Which form service?"]), ""),
        ],
    );
    let first = run_once(&cli, &asking).await;
    all_succeeded(&outcomes)?;
    expect_that!(asking.starts().len(), eq(1));
    let brief = &asking.starts()[0].brief;
    expect_that!(brief, contains_substring("github.com/ada/website"));
    expect_that!(brief, contains_substring("Add a contact page"));
    expect_that!(
        lines_with(&first.stdout, &note),
        elements_are![matches_regex(format!(
            r"^\d\d:\d\d  {note}  Add a contact page  Website  note → with context  asked  \d+m\d\ds  \$0\.11$"
        ))]
    );
    let waiting = server.read_item(&ada, HEAP, &note).await?;
    expect_that!(waiting["waits_for_you"], eq(true));
    expect_that!(waiting["description"], eq("A page with the address and a form."));

    // 5. Ada answers; the next run finishes, and the note is hers again with its context.
    reply(&cli, &format!("{note}.q1"), &["--comment", "Formspree"]).await?;
    let finishing = plays(
        &agent,
        &outcomes,
        0.22,
        vec![(args(&["work", "reached", "--result", "Wrote the context"]), "")],
    );
    run_once(&cli, &finishing).await;
    all_succeeded(&outcomes)?;
    let brief = &finishing.starts()[0].brief;
    expect_that!(brief, contains_substring("Which form service?"));
    expect_that!(brief, contains_substring("Formspree"));
    let with_context = server.read_item(&ada, HEAP, &note).await?;
    expect_that!(with_context["stage"], eq("with_context"));
    expect_that!(with_context["turn"], eq(&Value::Null));
    let after = run_once(&cli, &finishing).await;
    expect_that!(after.stdout, contains_regex(r"(?m)^\d\d:\d\d  Nothing to do\."));
    expect_that!(finishing.starts().len(), eq(1));

    // 6. Ada sends the note on; the agent splits it into two tasks, the second blocked by the first.
    let sent = send(&server, &ada, &note, "one", None).await?;
    expect_that!(sent["turn"]["goal"], eq("defined"));
    let splitting = {
        let (agent, outcomes) = (agent.clone(), outcomes.clone());
        let note = note.clone();
        StandIn::new(move |start: ToolStart| {
            let (agent, outcomes, note) = (agent.clone(), outcomes.clone(), note.clone());
            async move {
                let first = agent
                    .upheap_with_stdin(
                        &start,
                        &[
                            "item",
                            "add",
                            "Build the page",
                            "--task",
                            "--from",
                            &note,
                            "--need",
                            "expected",
                            "--age",
                            "keep",
                            "--context-file",
                            "-",
                            "--step",
                            "Write the markup",
                            "--step",
                            "Wire the form",
                            "-q",
                        ],
                        "The page itself.\n",
                    )
                    .await;
                let first_key = first.stdout.trim().to_owned();
                let second = agent
                    .upheap_with_stdin(
                        &start,
                        &[
                            "item",
                            "add",
                            "Announce the page",
                            "--task",
                            "--from",
                            &note,
                            "--need",
                            "expected",
                            "--age",
                            "keep",
                            "--context-file",
                            "-",
                            "--step",
                            "Write the post",
                            "--step",
                            "Publish it",
                            "--blocked-by",
                            &first_key,
                            "-q",
                        ],
                        "Tell people once it is live.\n",
                    )
                    .await;
                let became = agent.upheap(&start, &["item", "became", &note]).await;
                locked(&outcomes).extend([first, second, became]);
                Ok(ToolReport {
                    cost_usd: Some(0.33),
                    ..ToolReport::default()
                })
            }
        })
    };
    let split = run_once(&cli, &splitting).await;
    let made = std::mem::take(&mut *locked(&outcomes));
    for outcome in &made {
        expect_that!(outcome.exited(0), eq(true), "{}", outcome.stderr);
    }
    expect_that!(lines_with(&split.stdout, &note).len(), eq(1));
    let (build, announce) = (made[0].stdout.trim().to_owned(), made[1].stdout.trim().to_owned());
    let closed = server.read_item(&ada, HEAP, &note).await?;
    expect_that!(closed["state"], eq("became"));
    expect_that!(closed["turn"], eq(&Value::Null));
    for task in [&build, &announce] {
        let read = server.read_item(&ada, HEAP, task).await?;
        expect_that!(read["kind"], eq("task"));
        expect_that!(read["origin"]["key"], eq(note.as_str()));
        expect_that!(read["steps"].as_array().map(Vec::len), some(eq(2)));
        expect_that!(read["turn"], eq(&Value::Null));
        expect_that!(read["stage"], eq("defined"));
    }
    let blocked = server.read_item(&ada, HEAP, &announce).await?;
    expect_that!(blocked["state"], eq("blocked"));
    expect_that!(blocked["blocked_by"]["key"], eq(build.as_str()));

    // 7. Ada sends the first task to done. The agent starts it, ticks a step and asks Ada for something.
    let sent = send(&server, &ada, &build, "one", None).await?;
    expect_that!(sent["turn"]["goal"], eq("done"));
    let doing = plays(
        &agent,
        &outcomes,
        0.55,
        vec![
            (args(&["item", "start", &build]), ""),
            (args(&["step", "tick", &format!("{build}.1")]), ""),
            (args(&["question", "request", &build, "Create the form account"]), ""),
        ],
    );
    run_once(&cli, &doing).await;
    all_succeeded(&outcomes)?;
    let waiting = server.read_item(&ada, HEAP, &build).await?;
    expect_that!(waiting["state"], eq("in_progress"));
    expect_that!(waiting["step_progress"], eq(&json!({ "ticked": 1, "total": 2 })));
    expect_that!(waiting["waits_for_you"], eq(true));
    reply(&cli, &format!("{build}.q1"), &["--pick", "Done"]).await?;

    // 8. The next run ticks the rest and asks for review; Ada asks for changes; the agent asks again; Ada approves;
    // the agent proposes done; Ada agrees.
    let pr = "https://example.com/pr/1";
    let reviewing = plays(
        &agent,
        &outcomes,
        0.66,
        vec![
            (args(&["step", "tick", &format!("{build}.2")]), ""),
            (args(&["question", "review", &build, pr, "The contact page"]), ""),
        ],
    );
    run_once(&cli, &reviewing).await;
    all_succeeded(&outcomes)?;
    expect_that!(
        reviewing.starts()[0].brief,
        contains_substring("Create the form account")
    );
    let waiting = server.read_item(&ada, HEAP, &build).await?;
    expect_that!(waiting["waits_for_you"], eq(true));
    expect_that!(waiting["step_progress"], eq(&json!({ "ticked": 2, "total": 2 })));
    reply(
        &cli,
        &format!("{build}.q2"),
        &["--pick", "Changes asked", "--comment", "The form needs a label."],
    )
    .await?;
    let again = plays(
        &agent,
        &outcomes,
        0.77,
        vec![(
            args(&["question", "review", &build, pr, "The contact page, with a label"]),
            "",
        )],
    );
    run_once(&cli, &again).await;
    all_succeeded(&outcomes)?;
    let brief = &again.starts()[0].brief;
    expect_that!(brief, contains_substring("Review, round 1 · changes asked"));
    expect_that!(brief, contains_substring("The form needs a label."));
    let waiting = server.read_item(&ada, HEAP, &build).await?;
    expect_that!(waiting["waits_for_you"], eq(true));
    reply(&cli, &format!("{build}.q3"), &["--pick", "Approved"]).await?;
    let proposing = plays(
        &agent,
        &outcomes,
        0.88,
        vec![(args(&["question", "propose-done", &build, "The page is merged"]), "")],
    );
    run_once(&cli, &proposing).await;
    all_succeeded(&outcomes)?;
    let waiting = server.read_item(&ada, HEAP, &build).await?;
    expect_that!(waiting["waits_for_you"], eq(true));
    expect_that!(waiting["state"], eq("in_progress"));
    reply(&cli, &format!("{build}.q4"), &["--pick", "Close as done"]).await?;
    let done = server.read_item(&ada, HEAP, &build).await?;
    expect_that!(done["state"], eq("done"));
    let freed = server.read_item(&ada, HEAP, &announce).await?;
    expect_that!(freed["state"], eq("open"));

    // 9. Ada sends the second task all the way. Its tasks are born defined, so the turn goes straight to done: the
    // reach stays all_the_way across runs, and the item waits for Ada only at the review and the proposal.
    let sent = send(&server, &ada, &announce, "all_the_way", None).await?;
    expect_that!(sent["turn"]["goal"], eq("done"));
    let working = plays(
        &agent,
        &outcomes,
        0.99,
        vec![
            (args(&["item", "start", &announce]), ""),
            (args(&["step", "tick", &format!("{announce}.1")]), ""),
            (args(&["step", "tick", &format!("{announce}.2")]), ""),
            (
                args(&[
                    "question",
                    "review",
                    &announce,
                    "https://example.com/pr/2",
                    "The announcement",
                ]),
                "",
            ),
        ],
    );
    run_once(&cli, &working).await;
    all_succeeded(&outcomes)?;
    let waiting = server.read_item(&ada, HEAP, &announce).await?;
    expect_that!(waiting["waits_for_you"], eq(true));
    expect_that!(waiting["turn"]["reach"], eq("all_the_way"));
    reply(&cli, &format!("{announce}.q1"), &["--pick", "Approved"]).await?;
    let back = server.read_item(&ada, HEAP, &announce).await?;
    expect_that!(back["waits_for_you"], eq(false));
    expect_that!(back["turn"]["reach"], eq("all_the_way"));
    let closing = plays(
        &agent,
        &outcomes,
        1.11,
        vec![(
            args(&["question", "propose-done", &announce, "The announcement is out"]),
            "",
        )],
    );
    run_once(&cli, &closing).await;
    all_succeeded(&outcomes)?;
    let waiting = server.read_item(&ada, HEAP, &announce).await?;
    expect_that!(waiting["waits_for_you"], eq(true));
    reply(&cli, &format!("{announce}.q2"), &["--pick", "Close as done"]).await?;
    let done = server.read_item(&ada, HEAP, &announce).await?;
    expect_that!(done["state"], eq("done"));

    // 10. Ada can see what happened, when, and what each run cost: the runs of each item, newest first.
    let (header, lines) = runs_of(&cli, &note).await;
    expect_that!(header, eq("Runs 3 · $0.66"));
    expect_that!(
        lines,
        elements_are![
            matches_regex(run_line("with context → defined", "reached", "0.33")),
            matches_regex(run_line("as written → with context", "reached", "0.22")),
            matches_regex(run_line("as written → with context", "asked", "0.11")),
        ]
    );
    let (header, lines) = runs_of(&cli, &build).await;
    expect_that!(header, eq("Runs 4 · $2.86"));
    expect_that!(
        lines,
        elements_are![
            matches_regex(run_line("defined → done", "asked", "0.88")),
            matches_regex(run_line("defined → done", "asked", "0.77")),
            matches_regex(run_line("defined → done", "asked", "0.66")),
            matches_regex(run_line("defined → done", "asked", "0.55")),
        ]
    );
    let (header, lines) = runs_of(&cli, &announce).await;
    expect_that!(header, eq("Runs 2 · $2.10"));
    expect_that!(
        lines,
        elements_are![
            matches_regex(run_line("defined → done", "asked", "1.11")),
            matches_regex(run_line("defined → done", "asked", "0.99")),
        ]
    );
    Ok(())
}

/// The pattern of one run line in `item show`: the time it started, how long it took, the stages, how it ended, cost.
fn run_line(stages: &str, end: &str, cost: &str) -> String {
    format!(r"^  \d\d:\d\d  \d+m\d\ds  {stages} +{end} +\${cost}$")
}

/// The `Runs …` line of `item show <item>` and the run lines under it, without the result lines beneath each run
/// (they are indented further).
async fn runs_of(cli: &TestCli, item: &str) -> (String, Vec<String>) {
    let shown = cli.run(&["item", "show", item]).await;
    let mut lines = shown.stdout.lines().skip_while(|line| !line.starts_with("Runs "));
    let header = lines.next().unwrap_or_default().to_owned();
    let runs = lines
        .take_while(|line| !line.is_empty())
        .filter(|line| line.starts_with("  1") || line.starts_with("  0") || line.starts_with("  2"))
        .map(str::to_owned)
        .collect();
    (header, runs)
}
