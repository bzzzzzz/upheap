//! `upheap parameter promote <parameter>`: make a heap's parameter a team parameter, which the team's heaps use, and
//! merge the team's same-named heap parameters into it.

use api_types::parameters::{ParameterKindDto, ParameterListDto, TeamParameterDto};
use api_types::problem::ParameterMergeReachDto;
use clap::Args;

use crate::client::{ClientError, NO_QUERY};
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::error::shell_quoted;
use crate::lookup::{HeapHandle, find_parameter};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct PromoteArgs {
    /// The heap parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    /// Confirm the merge a refusal named: the token from its command line
    #[arg(long, value_name = "TOKEN")]
    pub confirm: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Makes the parameter its team's. The heap parameters of the team with its name merge into it, and that can't be
/// undone, so the first try is refused with what it would merge, even when that is nothing. Human output and
/// `--quiet` give the parameter's name, and `--json` the team parameter.
pub async fn run(args: PromoteArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let list = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
    let parameter = find_parameter(&args.parameter, &list.value, &heap)?;
    if parameter.kind == ParameterKindDto::Team {
        // Already promoted, by this command or another: repeating it is safe.
        match out.mode() {
            Mode::Json => out.json(&serde_json::to_string(parameter).map_err(std::io::Error::other)?)?,
            Mode::Quiet => out.line(&parameter.name)?,
            Mode::Human => out.line(&format!(
                "\"{}\" is already a team parameter of {}.",
                parameter.name, heap.team
            ))?,
        }
        return Ok(());
    }
    let id = parameter.id.to_string();
    let promoted = match api
        .post_confirming::<TeamParameterDto>(
            &["parameters", &id, "team"],
            &serde_json::json!({}),
            args.confirm.as_deref(),
        )
        .await
    {
        Ok(promoted) => promoted,
        Err(err) => return Err(refused(err, &args.parameter, &parameter.name, &heap)),
    };
    match out.mode() {
        Mode::Json => out.json(&promoted.body)?,
        Mode::Quiet => out.line(&promoted.value.name)?,
        Mode::Human => out.line(&format!(
            "Made \"{}\" a team parameter of {}.",
            promoted.value.name, heap.team
        ))?,
    }
    Ok(())
}

/// The refusal `err` of the write as the error to report: with the reach and the confirming command for
/// `parameter_merge_unconfirmed`. Any other refusal, such as `parameter_merge_linked`, stays as the server gave it.
fn refused(err: ClientError, typed: &str, name: &str, heap: &HeapHandle) -> CliError {
    let lines = match &err {
        ClientError::Problem { problem, .. } if problem.code == "parameter_merge_unconfirmed" => problem
            .parameter_merge_reach
            .as_ref()
            .map(|reach| merge_lines(reach, typed, name, heap)),
        _ => None,
    };
    match lines {
        Some(lines) => CliError::WriteRefused { lines, source: err },
        None => CliError::Client(err),
    }
}

/// The reach of a merge and how to go ahead with it.
fn merge_lines(reach: &ParameterMergeReachDto, typed: &str, name: &str, heap: &HeapHandle) -> Vec<String> {
    let slug_width = reach
        .entries
        .iter()
        .map(|entry| entry.heap.slug.chars().count())
        .max()
        .unwrap_or(0);
    let name_width = reach
        .entries
        .iter()
        .map(|entry| entry.parameter_name.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![format!(
        "error: Making \"{name}\" a team parameter of {} merges these heap parameters into it (parameter_merge_unconfirmed):",
        heap.team
    )];
    for entry in &reach.entries {
        let items = if entry.items == 1 {
            "1 item".to_owned()
        } else {
            format!("{} items", entry.items)
        };
        let adds = if entry.added_options.is_empty() {
            String::new()
        } else {
            format!("  adds {}", entry.added_options.join(", "))
        };
        let marks = match (entry.starting, entry.archived) {
            (true, true) => "  (this heap, archived)",
            (true, false) => "  (this heap)",
            (false, true) => "  (archived)",
            (false, false) => "",
        };
        lines.push(format!(
            "  {:<slug_width$}  {:<name_width$}  {items}{adds}{marks}",
            entry.heap.slug, entry.parameter_name
        ));
    }
    lines.push(
        "Each heap keeps its weight. Rankings can shift in a heap whose options were in another order.".to_owned(),
    );
    lines.push("This can't be undone. To go ahead, run:".to_owned());
    lines.push(format!("  {}", command(typed, heap, &reach.token)));
    lines
}

/// The command that repeats the write with `token`. It names the heap, so it doesn't depend on the default heap, and
/// puts a name that starts with `-` after `--`.
fn command(typed: &str, heap: &HeapHandle, token: &str) -> String {
    let flags = format!("--heap {heap} --confirm {}", shell_quoted(token));
    if typed.starts_with('-') {
        format!("upheap parameter promote {flags} -- {}", shell_quoted(typed))
    } else {
        format!("upheap parameter promote {} {flags}", shell_quoted(typed))
    }
}
