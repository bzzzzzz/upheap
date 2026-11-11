//! The work commands, for the agent's queue and its runs: their args and flows, and what they share.
//!
//! `next` and `claim` look for work in some heaps; `reached` and `finish` belong to a run, named by `UPHEAP_RUN`,
//! which `claim` prints for a script (or, later, `work run`) to export.

pub mod claim;
pub mod finish;
pub mod next;
pub mod reached;

use api_types::common::KindDto;
use api_types::me::MeDto;
use api_types::turns::GoalDto;
use api_types::work::WorkItemDto;
use clap::Args;

use crate::client::{ApiClient, ClientError, NO_QUERY};
use crate::commands::UsageError;
use crate::lookup::{HeapName, resolve_heap};
use crate::{App, CliError};

/// The flag of the commands that look for work: repeatable.
#[derive(Args, Clone, Debug)]
pub struct HeapsArgs {
    /// A heap to look for work in: team/heap, or a heap slug only one of your teams has. Repeat for more. Left
    /// out, the default heap (UPHEAP_HEAP, or `upheap heap use`)
    #[arg(long = "heap", value_name = "HEAP")]
    pub names: Vec<String>,
}

/// The heaps `args` names, each once as `team/heap`, found among the caller's. With none named, the default heap.
pub(super) async fn resolve_heaps(
    args: &HeapsArgs,
    app: &App,
    client: &ApiClient,
    token: &str,
) -> Result<Vec<String>, CliError> {
    let typed: Vec<&str> = if args.names.is_empty() {
        vec![app.heap_name(None).ok_or(crate::config::SettingError::NoHeap)?]
    } else {
        args.names.iter().map(String::as_str).collect()
    };
    // Looked up among the caller's heaps, even when written in full, so a heap that isn't theirs is told apart from
    // a server that has no agents (which answers `404` for every `/work` route).
    let me = client.get::<MeDto>(&["me"], NO_QUERY, token).await?;
    let mut heaps: Vec<String> = Vec::new();
    for name in typed {
        let handle = resolve_heap(HeapName::parse(name)?, &me.value)?.to_string();
        if !heaps.contains(&handle) {
            heaps.push(handle);
        }
    }
    Ok(heaps)
}

/// The run `UPHEAP_RUN` names, for the commands that belong to one.
pub(super) fn current_run<'a>(app: &'a App, verb: &'static str) -> Result<&'a str, UsageError> {
    app.env.run.as_deref().ok_or(UsageError::NeedsRun { verb })
}

/// A `404` from a `/work` route means the server doesn't run agents (or, for a run's own routes, doesn't know the
/// run).
pub(super) fn missing_agents(err: ClientError, run: Option<&str>) -> CliError {
    match err {
        ClientError::Problem { ref problem, .. } if problem.status == 404 => match run {
            Some(run) => CliError::NoSuchRun { run: run.to_owned() },
            None => CliError::NoAgents,
        },
        other => other.into(),
    }
}

/// `note → with context`: where the item is going.
pub(super) fn journey(item: &WorkItemDto) -> String {
    let kind = match item.kind {
        KindDto::Note => "note",
        KindDto::Task => "task",
        KindDto::Unknown => "item",
    };
    format!("{kind} → {}", goal_phrase(item.goal))
}

pub(super) const fn goal_phrase(goal: GoalDto) -> &'static str {
    match goal {
        GoalDto::WithContext => "with context",
        GoalDto::Defined => "defined",
        GoalDto::Done => "done",
        GoalDto::Unknown => "a state this version doesn't know",
    }
}

/// `1 item waits for you.` or `N items wait for you.`, or nothing at 0.
pub(super) fn waiting_text(count: u32) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 item waits for you.".to_owned()),
        n => Some(format!("{n} items wait for you.")),
    }
}
