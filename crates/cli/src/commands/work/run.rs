//! `upheap work run --tool <tool> [--heap <heap>]... [--once]`: the agent loop.
//!
//! It claims the next item of the queue, starts the agent tool on it with a brief, keeps the claim alive, records
//! how the run ended and what it cost, and waits when there is nothing to do. The server decides how a run ended
//! (it reached its goal, it asked, or it failed); the loop only reports what the tool did. The tool sits behind
//! [`AgentTools`], and the waits and interruptions come in through [`WorkDeps`], so tests pass their own.

use std::time::Duration;

use api_types::common::ItemDto;
use api_types::heaps::HeapDto;
use api_types::work::{ClaimDto, ClaimRequestDto, FinishRequestDto, RunDto, RunEndDto};
use clap::Args;
use reqwest::Url;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, MissedTickBehavior};

use super::brief::{BriefParts, KnownGoal, build_brief};
use super::{HeapsArgs, NamedHeap, end_word, journey, missing_agents, resolve_named_heaps, waiting_text};
use crate::agent::{AgentTools, Interrupt, Tool, ToolError, ToolReport, ToolStart, WorkDeps};
use crate::client::{ClientError, Fetched, NO_QUERY};
use crate::commands::heap_api::HeapApi;
use crate::commands::item::show::item_text;
use crate::commands::{SharedArgs, UsageError};
use crate::lookup::HeapHandle;
use crate::output::{Cell, Mode, Output, Tone, shorten};
use crate::{App, CliError};

/// How many times the end of a run is sent before giving up on the server being out of reach.
const FINISH_TRIES: u32 = 3;

/// The longest title a run's line shows, in characters.
const TITLE_WIDTH: usize = 40;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The agent tool to start on each item
    #[arg(long, value_enum)]
    pub tool: Tool,
    #[command(flatten)]
    pub heaps: HeapsArgs,
    /// Do one run, or nothing when there is nothing to do, and exit
    #[arg(long)]
    pub once: bool,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Works the queue until stopped: claims an item, starts the tool on it, and records how the run ended. Prints a line
/// for each run that ends, `14:02  k3x9a  Rotate the certs  note → with context  reached  2m10s  $0.31`, and one
/// when nothing is left to do. Ctrl-C once stops the loop after the current run, twice stops the run too. `--quiet`
/// prints the key of each item worked, `--json` each run as the server records its end. Exit 0 when stopped or
/// `--once` is done, and 5 when the server can't be reached with `--once`. Refused inside a run.
pub async fn run(args: RunArgs, app: &App, work: WorkDeps, out: &mut Output) -> Result<(), CliError> {
    if app.env.run.is_some() {
        return Err(UsageError::RunInRun.into());
    }
    let server = app.server(&args.shared)?;
    let client = app.client(server.clone())?;
    let token = app.access_token(&client).await?;
    let heaps = resolve_named_heaps(&args.heaps, app, &client, &token).await?;
    work.tools.check(args.tool)?;
    if out.mode() == Mode::Human {
        out.line(&header(&heaps, args.tool, args.once))?;
    }
    let WorkDeps {
        tools,
        interrupts,
        idle_wait,
        heartbeat_every,
        finish_retry,
    } = work;
    let mut worker = RunLoop {
        app,
        server,
        tool: args.tool,
        heaps: heaps.into_iter().map(|heap| heap.handle).collect(),
        once: args.once,
        tools,
        interrupts: interrupts(),
        idle_wait,
        heartbeat_every,
        finish_retry,
        stopping: false,
        idle_noted: None,
        unreachable_noted: false,
    };
    worker.work_the_queue(out).await
}

/// `Working in Work and House with claude. Ctrl-C stops after the current run.`
fn header(heaps: &[NamedHeap], tool: Tool, once: bool) -> String {
    let names: Vec<&str> = heaps.iter().map(|heap| heap.name.as_str()).collect();
    let mut text = format!("Working in {} with {tool}.", join_names(&names));
    if !once {
        text.push_str(" Ctrl-C stops after the current run.");
    }
    text
}

/// `A`, `A and B`, `A, B and C`.
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// What the loop does next.
enum Next {
    /// Claim again at once.
    Again,
    /// Wait before claiming again.
    Wait,
    /// End the loop.
    Stop,
}

/// What the loop does when a claim, or the reads that follow it, fail.
#[derive(Debug, PartialEq, Eq)]
enum AfterFailure {
    /// Say the server can't be reached, wait, and ask again.
    TryAgain,
    /// End the loop with the error.
    Stop,
}

/// A server that can't be reached is waited for, unless the person asked for one run; any other failure ends the
/// loop, since asking again would only fail again.
fn after_failure(err: &CliError, once: bool) -> AfterFailure {
    if is_unreachable(err) && !once {
        AfterFailure::TryAgain
    } else {
        AfterFailure::Stop
    }
}

/// Whether the failure is the server being out of reach or unavailable (exit code 5).
fn is_unreachable(err: &CliError) -> bool {
    err.exit_code() == 5
}

/// How the tool's time ended, as far as the loop can tell.
enum Driven {
    /// The tool ended on its own, or could not be started.
    Ended(Result<ToolReport, ToolError>),
    /// The server says the run is over: the person took the turn back, or the run lapsed. The tool was stopped.
    Over,
    /// The person stopped the run. The tool was stopped.
    Stopped,
}

/// The loop's context: what it works on, how, and what it has said so far.
struct RunLoop<'a> {
    app: &'a App,
    server: Url,
    tool: Tool,
    /// The heaps the queue is over, each as `team/heap`.
    heaps: Vec<String>,
    once: bool,
    tools: Box<dyn AgentTools>,
    interrupts: UnboundedReceiver<Interrupt>,
    idle_wait: Duration,
    heartbeat_every: Duration,
    finish_retry: Duration,
    /// The person pressed Ctrl-C once: the loop ends after the run it is in.
    stopping: bool,
    /// How many items waited for the person when the idle line was last printed, while the loop stays idle.
    idle_noted: Option<u32>,
    /// Whether the server being out of reach has been said, while it stays so.
    unreachable_noted: bool,
}

impl RunLoop<'_> {
    async fn work_the_queue(&mut self, out: &mut Output) -> Result<(), CliError> {
        loop {
            if self.is_interrupted() {
                return Ok(());
            }
            let next = match self.work_a_pass(out).await {
                Ok(next) => next,
                Err(err) if after_failure(&err, self.once) == AfterFailure::TryAgain => {
                    self.note_unreachable(out);
                    Next::Wait
                }
                Err(err) => return Err(err),
            };
            match next {
                Next::Stop => return Ok(()),
                _ if self.once => return Ok(()),
                Next::Again => {}
                Next::Wait => {
                    if !self.wait_to_ask_again().await {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Claims once, and works the run that came of it.
    async fn work_a_pass(&mut self, out: &mut Output) -> Result<Next, CliError> {
        let claimed = self.claim().await?;
        self.unreachable_noted = false;
        let Some(run) = claimed.value.run else {
            self.note_idle(out, claimed.value.waiting_count)?;
            return Ok(Next::Wait);
        };
        self.idle_noted = None;
        let Some(goal) = KnownGoal::from_dto(run.item.goal) else {
            let failure = "This version of upheap doesn't know this turn's goal. Update upheap.";
            let request = FinishRequestDto {
                cost_usd: None,
                failure: Some(failure.to_owned()),
            };
            self.finish_and_print(out, &run, &request).await?;
            return Ok(Next::Wait);
        };
        self.work_one(out, run, goal).await
    }

    async fn claim(&self) -> Result<Fetched<ClaimDto>, CliError> {
        let client = self.app.client(self.server.clone())?;
        let token = self.app.access_token(&client).await?;
        let body = ClaimRequestDto {
            heaps: self.heaps.clone(),
        };
        client
            .post::<ClaimDto>(&["work", "runs"], &body, &token)
            .await
            .map_err(|err| missing_agents(err, None))
    }

    /// Runs the tool on the claimed `run`, keeps the claim alive meanwhile, and records how it ended.
    async fn work_one(&mut self, out: &mut Output, run: RunDto, goal: KnownGoal) -> Result<Next, CliError> {
        self.note_started(out, &run);
        let handle = HeapHandle {
            team: run.item.heap.team_slug.clone(),
            heap: run.item.heap.slug.clone(),
        };
        let (heap, item) = match self.read_heap_and_item(&run, &handle).await {
            Ok(read) => read,
            Err(err) => {
                let request = FinishRequestDto {
                    cost_usd: None,
                    failure: Some(format!("Couldn't read the item before starting: {err}")),
                };
                self.finish_and_print(out, &run, &request).await?;
                return Err(err);
            }
        };
        let brief = build_brief(&BriefParts {
            goal,
            heap_name: &run.item.heap.name,
            heap_handle: &handle.to_string(),
            heap_context: heap.context.as_deref(),
            item_text: &item_text(&item, self.app.now()),
            line: run.item.line.as_deref(),
            runs: run.item.runs,
            failed_runs: run.item.failed_runs,
        });
        let start = ToolStart {
            tool: self.tool,
            access: goal.access(),
            brief,
            env: vec![
                ("UPHEAP_RUN".to_owned(), run.id.to_string()),
                ("UPHEAP_SERVER".to_owned(), self.server.to_string()),
                ("UPHEAP_HEAP".to_owned(), handle.to_string()),
            ],
        };
        tracing::info!(run_id = %run.id, item = %run.item.key, goal = ?goal, "starting the tool");
        let driven = self.drive_tool(out, &run, start).await;
        let (request, not_started) = match driven {
            Driven::Ended(Ok(report)) => {
                tracing::debug!(run_id = %run.id, last_message = ?report.last_message, "the tool ended");
                let request = FinishRequestDto {
                    cost_usd: report.cost_usd,
                    failure: report.failure,
                };
                (request, None)
            }
            Driven::Ended(Err(err)) => {
                let request = FinishRequestDto {
                    cost_usd: None,
                    failure: Some(err.failure_text()),
                };
                (request, Some(err))
            }
            Driven::Over => (FinishRequestDto::default(), None),
            Driven::Stopped => {
                let request = FinishRequestDto {
                    cost_usd: None,
                    failure: Some("Stopped by the person.".to_owned()),
                };
                (request, None)
            }
        };
        self.finish_and_print(out, &run, &request).await?;
        // Every run would fail the same way, so the loop does not go on.
        if let Some(err) = not_started {
            return Err(err.into());
        }
        Ok(if self.stopping { Next::Stop } else { Next::Again })
    }

    /// The heap, for its context, and the item, as they are now.
    async fn read_heap_and_item(&self, run: &RunDto, handle: &HeapHandle) -> Result<(HeapDto, ItemDto), CliError> {
        let client = self.app.client(self.server.clone())?;
        let token = self.app.access_token(&client).await?;
        let api = HeapApi {
            client: &client,
            token: &token,
            heap: handle,
        };
        let heap = api.get::<HeapDto>(&[], NO_QUERY).await?;
        // A token for each request, so a long loop never keeps one past its expiry.
        let token = self.app.access_token(&client).await?;
        let api = HeapApi {
            client: &client,
            token: &token,
            heap: handle,
        };
        let item = api.get_item(&run.item.key, &run.item.key).await?;
        Ok((heap.value, item.value))
    }

    /// Waits for the tool, and while it runs says every `heartbeat_every` that the run is alive. Gives up the tool
    /// (which stops it) when the server says the run is over, or on the person's second Ctrl-C or a request to
    /// stop now. The first Ctrl-C only notes that the loop will end after this run.
    async fn drive_tool(&mut self, out: &mut Output, run: &RunDto, start: ToolStart) -> Driven {
        let mut tool = self.tools.run(start);
        let mut heartbeat = tokio::time::interval_at(Instant::now() + self.heartbeat_every, self.heartbeat_every);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut listening = true;
        loop {
            tokio::select! {
                result = &mut tool => return Driven::Ended(result),
                _ = heartbeat.tick() => {
                    if self.is_run_over(run).await {
                        return Driven::Over;
                    }
                }
                interrupt = self.interrupts.recv(), if listening => match interrupt {
                    None => listening = false,
                    Some(Interrupt::CtrlC) if !self.stopping => {
                        self.stopping = true;
                        // Nowhere left to say it if stderr is gone.
                        let _ = out.note("Stopping after this run. Ctrl-C again stops it now.");
                    }
                    Some(_) => {
                        self.stopping = true;
                        return Driven::Stopped;
                    }
                },
            }
        }
    }

    /// Sends a heartbeat. True when the server says the run is over; any other failure is left for the next tick,
    /// since the agent's own writes keep the claim alive too.
    async fn is_run_over(&self, run: &RunDto) -> bool {
        let id = run.id.to_string();
        let sent = async {
            let client = self.app.client(self.server.clone()).map_err(CliError::from)?;
            let client = client.in_run(Some(id.clone()));
            let token = self.app.access_token(&client).await?;
            client
                .post_empty(&["work", "runs", &id, "heartbeat"], &token)
                .await
                .map_err(CliError::from)
        };
        match sent.await {
            Err(CliError::Client(ClientError::Problem { problem, .. })) => problem.code == "run_over",
            Ok(()) | Err(_) => false,
        }
    }

    /// Records the end of `run` and prints its line. When the end can't be recorded: with `--once` and the server out
    /// of reach, the error ends the command; otherwise a warning says so, and the run lapses on the server.
    async fn finish_and_print(
        &self,
        out: &mut Output,
        run: &RunDto,
        request: &FinishRequestDto,
    ) -> Result<(), CliError> {
        match self.finish_with_retry(run, request).await {
            Ok(finished) => {
                tracing::info!(run_id = %run.id, end = ?finished.value.end, "run ended");
                self.print_run(out, &finished).map_err(CliError::from)
            }
            Err(err) if self.once && is_unreachable(&err) => Err(err),
            Err(err) => {
                // Nowhere left to say it if stderr is gone.
                let _ = out.note(&format!("warning: couldn't record the end of run {}: {err}", run.id));
                Ok(())
            }
        }
    }

    /// Sends the end of the run, trying again `finish_retry` later when the server can't be reached.
    async fn finish_with_retry(&self, run: &RunDto, request: &FinishRequestDto) -> Result<Fetched<RunDto>, CliError> {
        let mut tries = 1;
        loop {
            match self.finish(run, request).await {
                Err(err) if is_unreachable(&err) && tries < FINISH_TRIES => {
                    tries += 1;
                    tokio::time::sleep(self.finish_retry).await;
                }
                result => return result,
            }
        }
    }

    async fn finish(&self, run: &RunDto, request: &FinishRequestDto) -> Result<Fetched<RunDto>, CliError> {
        let id = run.id.to_string();
        let client = self.app.client(self.server.clone())?.in_run(Some(id.clone()));
        let token = self.app.access_token(&client).await?;
        client
            .post::<RunDto>(&["work", "runs", &id, "finish"], request, &token)
            .await
            .map_err(|err| missing_agents(err, Some(&id)))
    }

    /// What a run that ended prints on stdout: its line, the key with `--quiet`, or the server's body with `--json`.
    fn print_run(&self, out: &mut Output, finished: &Fetched<RunDto>) -> std::io::Result<()> {
        let run = &finished.value;
        match out.mode() {
            Mode::Json => out.json(&finished.body),
            Mode::Quiet => out.line(&run.item.key),
            Mode::Human => {
                let tone = if run.end == Some(RunEndDto::Failed) {
                    Tone::Alert
                } else {
                    Tone::Plain
                };
                let mut row = vec![
                    Cell::toned(self.local_time(run.started_at), Tone::Dim),
                    Cell::toned(run.item.key.clone(), Tone::Strong),
                    Cell::new(shorten(&run.item.title, TITLE_WIDTH)),
                    Cell::new(journey(&run.item)),
                    Cell::toned(end_word(run.end).unwrap_or("ended"), tone),
                ];
                if let Some(ended_at) = run.ended_at {
                    row.push(Cell::new(format_duration(ended_at - run.started_at)));
                }
                if let Some(cost) = run.cost_usd {
                    row.push(Cell::new(format!("${cost:.2}")));
                }
                for line in out.render_table(&[row]) {
                    out.table_line("", &line)?;
                }
                if run.end == Some(RunEndDto::Failed)
                    && let Some(failure) = run.failure.as_deref()
                {
                    out.line(&format!("       {}", one_line(failure)))?;
                }
                Ok(())
            }
        }
    }

    /// `14:02  k3x9a  Rotate the certs  note → with context  started`, on stderr.
    fn note_started(&self, out: &mut Output, run: &RunDto) {
        if out.mode() != Mode::Human {
            return;
        }
        let line = format!(
            "{}  {}  {}  {}  started",
            self.local_time(run.started_at),
            run.item.key,
            shorten(&run.item.title, TITLE_WIDTH),
            journey(&run.item)
        );
        // Nowhere left to say it if stderr is gone.
        let _ = out.note_toned(&line, Tone::Dim);
    }

    /// `14:06  Nothing to do. 1 item waits for you. Asking again in a minute.`, once for each stretch of idling: again
    /// only when the number of items that wait for the person changes.
    fn note_idle(&mut self, out: &mut Output, waiting: u32) -> std::io::Result<()> {
        if self.idle_noted == Some(waiting) {
            return Ok(());
        }
        self.idle_noted = Some(waiting);
        if out.mode() != Mode::Human {
            return Ok(());
        }
        let mut text = String::from("Nothing to do.");
        if let Some(waiting) = waiting_text(waiting) {
            text.push(' ');
            text.push_str(&waiting);
        }
        if !self.once {
            text.push_str(" Asking again in a minute.");
        }
        let now = self.local_time(self.app.now().as_millisecond());
        out.line_parts(&[
            (now.as_str(), Tone::Dim),
            ("  ", Tone::Plain),
            (text.as_str(), Tone::Plain),
        ])
    }

    /// `14:07  Can't reach the server. Trying again in a minute.`, on stderr, once for each stretch of it.
    fn note_unreachable(&mut self, out: &mut Output) {
        if std::mem::replace(&mut self.unreachable_noted, true) {
            return;
        }
        let now = self.local_time(self.app.now().as_millisecond());
        // Nowhere left to say it if stderr is gone.
        let _ = out.note(&format!("{now}  Can't reach the server. Trying again in a minute."));
    }

    /// Whether the person interrupted while nothing was running: the loop ends.
    fn is_interrupted(&mut self) -> bool {
        self.interrupts.try_recv().is_ok()
    }

    /// Waits `idle_wait` before asking again. False when the person interrupted, which ends the loop at once.
    async fn wait_to_ask_again(&mut self) -> bool {
        match tokio::time::timeout(self.idle_wait, self.interrupts.recv()).await {
            Err(_elapsed) => true,
            Ok(Some(_)) => false,
            // Nobody can interrupt any more.
            Ok(None) => {
                tokio::time::sleep(self.idle_wait).await;
                true
            }
        }
    }

    /// `ms`, UTC milliseconds, as `14:02` in the machine's time zone, or UTC without one.
    fn local_time(&self, ms: i64) -> String {
        local_time(ms, self.app.env.time_zone.as_deref())
    }
}

/// `ms`, UTC milliseconds, as `HH:MM` in the time zone named by `zone`, or in UTC when there is none or it is unknown.
fn local_time(ms: i64, zone: Option<&str>) -> String {
    let zone = zone
        .and_then(|name| jiff::tz::TimeZone::get(name).ok())
        .unwrap_or(jiff::tz::TimeZone::UTC);
    match jiff::Timestamp::from_millisecond(ms) {
        Ok(at) => at.to_zoned(zone).strftime("%H:%M").to_string(),
        Err(_) => "--:--".to_owned(),
    }
}

/// How long a run took, from its length in milliseconds: `0m40s`, `2m10s`, `1h05m`.
fn format_duration(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    if seconds < 3600 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{:02}m", seconds / 3600, seconds % 3600 / 60)
    }
}

/// `text` on one line, so it can't forge another row.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn unreachable_error() -> CliError {
        // The cheapest `reqwest::Error` to make: a request to an address that isn't one.
        let source = reqwest::Client::new().get("not a url").build().map(|_| ()).unwrap_err();
        CliError::Client(ClientError::Unreachable {
            server: "http://127.0.0.1:1".to_owned(),
            source,
        })
    }

    #[gtest]
    fn a_server_out_of_reach_is_waited_for_unless_one_run_was_asked_for() {
        let unreachable = unreachable_error();

        expect_that!(after_failure(&unreachable, false), eq(&AfterFailure::TryAgain));
        expect_that!(after_failure(&unreachable, true), eq(&AfterFailure::Stop));
    }

    #[gtest]
    fn every_other_failure_ends_the_loop() {
        for err in [CliError::NoAgents, CliError::NoSuchRun { run: "x".to_owned() }] {
            expect_that!(after_failure(&err, false), eq(&AfterFailure::Stop));
            expect_that!(after_failure(&err, true), eq(&AfterFailure::Stop));
        }
    }

    #[gtest]
    fn heap_names_are_joined_with_commas_and_a_final_and() {
        expect_that!(join_names(&[]), eq(""));
        expect_that!(join_names(&["Work"]), eq("Work"));
        expect_that!(join_names(&["Work", "House"]), eq("Work and House"));
        expect_that!(join_names(&["Work", "House", "Garden"]), eq("Work, House and Garden"));
    }

    #[gtest]
    fn the_header_leaves_the_ctrl_c_sentence_out_with_once() {
        let heaps = vec![
            NamedHeap {
                handle: "ada/work".to_owned(),
                name: "Work".to_owned(),
            },
            NamedHeap {
                handle: "ada/house".to_owned(),
                name: "House".to_owned(),
            },
        ];

        expect_that!(
            header(&heaps, Tool::Claude, false),
            eq("Working in Work and House with claude. Ctrl-C stops after the current run.")
        );
        expect_that!(
            header(&heaps, Tool::Claude, true),
            eq("Working in Work and House with claude.")
        );
    }

    #[gtest]
    fn a_duration_is_minutes_and_seconds_and_then_hours_and_minutes() {
        expect_that!(format_duration(0), eq("0m00s"));
        expect_that!(format_duration(40_000), eq("0m40s"));
        expect_that!(format_duration(130_000), eq("2m10s"));
        expect_that!(format_duration(3_599_999), eq("59m59s"));
        expect_that!(format_duration(3_900_000), eq("1h05m"));
        expect_that!(format_duration(-5), eq("0m00s"));
    }

    #[gtest]
    fn times_are_local_hours_and_minutes() {
        // 2027-01-01T12:00:00Z
        let noon = 1_798_804_800_000;

        expect_that!(local_time(noon, None), eq("12:00"));
        expect_that!(local_time(noon, Some("Europe/Berlin")), eq("13:00"));
        expect_that!(local_time(noon, Some("Not/AZone")), eq("12:00"));
    }

    #[gtest]
    fn a_failure_is_shown_on_one_line() {
        expect_that!(
            one_line("claude exited with code 1:\n  boom\r\n"),
            eq("claude exited with code 1: boom")
        );
    }
}
