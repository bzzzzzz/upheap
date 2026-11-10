//! `upheap item show <item>`: one item in full, and with `--history` the changes made to it.

use api_types::audit::{AuditLogDto, AuditQueryDto};
use api_types::common::{ItemDto, KindDto, NeedDto, StageDto, StepStateDto};
use clap::Args;

use super::common::{locate_item, state_text};
use crate::commands::heap_api::{HeapApi, indented_context};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// Add the changes made to the item, newest first
    #[arg(long)]
    pub history: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints the item. `--quiet` prints its key, and neither it nor `--json` without `--history` reads the history.
pub async fn run(args: ShowArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&args.item, &at.key).await?;
    let history = if args.history && out.mode() != Mode::Quiet {
        let query = AuditQueryDto {
            entity_id: Some(item.value.id),
            ..AuditQueryDto::default()
        };
        Some(api.get::<AuditLogDto>(&["audit"], &query).await?)
    } else {
        None
    };
    match (out.mode(), history) {
        (Mode::Quiet, _) => out.line(&item.value.key)?,
        (Mode::Json, None) => out.json(&item.body)?,
        // Both bodies were read as JSON already, so they make one JSON object as they came.
        (Mode::Json, Some(history)) => out.json(&format!(
            "{{\"item\":{},\"history\":{}}}",
            item.body.trim(),
            history.body.trim()
        ))?,
        (Mode::Human, history) => {
            render_item(&item.value, out)?;
            if let Some(history) = history {
                render_history(&history.value, out)?;
            }
        }
    }
    Ok(())
}

fn render_item(item: &ItemDto, out: &mut Output) -> std::io::Result<()> {
    let head = [vec![
        Cell::toned(item.key.clone(), Tone::Strong),
        Cell::new(item.title.clone()),
    ]];
    for line in out.render_table(&head) {
        out.table_line("", &line)?;
    }
    let lines = out.render_table(&fields(item));
    for line in &lines {
        out.table_line("", line)?;
    }
    if let Some(description) = item.description.as_deref().filter(|text| !text.is_empty()) {
        out.line("")?;
        out.line_toned("Context", Tone::Strong)?;
        for line in indented_context(description) {
            out.line(&line)?;
        }
    }
    render_steps(item, out)
}

/// The task's steps, when it has any: a heading with the progress, and a line for each step in order, with its
/// context under it. Dropped steps say so; they are not in the progress.
fn render_steps(item: &ItemDto, out: &mut Output) -> std::io::Result<()> {
    // `null` (a list row) reads as no steps; `item show` reads a single item, which always has the array.
    let steps = item.steps.as_deref().unwrap_or_default();
    if steps.is_empty() {
        return Ok(());
    }
    out.line("")?;
    match &item.step_progress {
        Some(progress) => out.line_toned(&format!("Steps {}/{}", progress.ticked, progress.total), Tone::Strong)?,
        None => out.line_toned("Steps", Tone::Strong)?,
    }
    let names: Vec<String> = steps
        .iter()
        .map(|step| format!("{}.{}", item.key, step.number))
        .collect();
    let width = names.iter().map(|name| name.chars().count()).max().unwrap_or(0);
    for (step, name) in steps.iter().zip(&names) {
        let mark = match step.state {
            StepStateDto::Open => "[ ]",
            StepStateDto::Ticked => "[x]",
            StepStateDto::Dropped => "[-]",
            StepStateDto::Unknown => "[?]",
        };
        let line = format!("{mark} {name:<width$}  {}", step.title);
        match step.state {
            StepStateDto::Dropped => out.line_parts(&[(line.trim_end(), Tone::Plain), ("  dropped", Tone::Dim)])?,
            StepStateDto::Open | StepStateDto::Ticked | StepStateDto::Unknown => out.line(&line)?,
        }
        for context_line in step.context.iter().flat_map(|context| context.lines()) {
            out.line(&format!("        {}", context_line.replace('\t', "    ")))?;
        }
    }
    Ok(())
}

/// The item's labelled fields, as rows of label and value. Unset sizes, due time, tags and the rest are left out.
fn fields(item: &ItemDto) -> Vec<Vec<Cell>> {
    let mut rows = vec![vec![Cell::new("Kind"), Cell::new(kind_text(item.kind))]];
    if let Some(stage) = item.stage {
        rows.push(vec![Cell::new("Stage"), Cell::new(stage_text(stage))]);
    }
    rows.push(vec![Cell::new("State"), Cell::new(state_text(item.state))]);
    rows.push(vec![Cell::new("Need"), Cell::new(need_text(item.need))]);
    if let Some(effort) = item.effort {
        rows.push(vec![Cell::new("Effort"), Cell::new(size_text(effort))]);
    }
    if let Some(impact) = item.impact {
        rows.push(vec![Cell::new("Impact"), Cell::new(size_text(impact))]);
    }
    if let Some(due) = &item.due {
        let label = if due.firm {
            format!("{} (firm)", due.label)
        } else {
            due.label.clone()
        };
        let tone = if due.overdue { Tone::Alert } else { Tone::Plain };
        rows.push(vec![Cell::new("Due"), Cell::toned(label, tone)]);
    }
    if !item.tags.is_empty() {
        rows.push(vec![Cell::new("Tags"), Cell::new(super::common::tags_text(item))]);
    }
    if let Some(blocker) = &item.blocked_by {
        rows.push(vec![
            Cell::new("Blocked by"),
            Cell::new(format!(
                "{}  {} ({})",
                blocker.key,
                blocker.title,
                state_text(blocker.state)
            )),
        ]);
    }
    for parameter in &item.parameters {
        let Some(option) = &parameter.option else {
            continue;
        };
        let archived = if option.archived || parameter.parameter_archived {
            " (archived)"
        } else {
            ""
        };
        rows.push(vec![
            Cell::new(parameter.name.clone()),
            Cell::new(format!("{}{archived}", option.label)),
        ]);
    }
    rows.push(vec![
        Cell::new("Created"),
        Cell::new(with_author(&item.created_label, item.created_by.as_ref())),
    ]);
    if let Some(label) = &item.closed_label {
        rows.push(vec![
            Cell::new("Closed"),
            Cell::new(with_author(label, item.closed_by.as_ref())),
        ]);
    }
    rows
}

/// `label`, followed by `by <name>` when the server says who.
fn with_author(label: &str, by: Option<&api_types::common::UserRefDto>) -> String {
    match by {
        Some(user) => format!("{label} by {}", user.display_name),
        None => label.to_owned(),
    }
}

fn kind_text(kind: KindDto) -> &'static str {
    match kind {
        KindDto::Note => "note",
        KindDto::Task => "task",
        KindDto::Unknown => "unknown",
    }
}

fn stage_text(stage: StageDto) -> &'static str {
    match stage {
        StageDto::AsWritten => "as written",
        StageDto::WithContext => "with context",
        StageDto::Defined => "defined",
        StageDto::Unknown => "unknown",
    }
}

fn need_text(need: NeedDto) -> &'static str {
    match need {
        NeedDto::Idea => "idea",
        NeedDto::NiceToHave => "nice to have",
        NeedDto::Expected => "expected",
        NeedDto::Critical => "critical",
    }
}

fn size_text(size: api_types::common::SizeDto) -> &'static str {
    use api_types::common::SizeDto;
    match size {
        SizeDto::S => "s",
        SizeDto::M => "m",
        SizeDto::L => "l",
        SizeDto::Xl => "xl",
    }
}

/// The history as the server wrote it: a line for each entry, newest first, and under it a line for each change.
fn render_history(log: &AuditLogDto, out: &mut Output) -> std::io::Result<()> {
    out.line("")?;
    out.line_toned("History", Tone::Strong)?;
    let rows: Vec<Vec<Cell>> = log
        .entries
        .iter()
        .map(|entry| {
            vec![
                Cell::toned(entry.created_label.clone(), Tone::Dim),
                Cell::new(format!("{} {}", entry.actor.display_name, entry.phrase)),
            ]
        })
        .collect();
    let lines = out.render_table(&rows);
    for (entry, line) in log.entries.iter().zip(&lines) {
        out.table_line("", line)?;
        for change in &entry.changes {
            out.line(&format!(
                "  {}: {} → {}",
                change.label,
                change.before.as_deref().unwrap_or("none"),
                change.after.as_deref().unwrap_or("none")
            ))?;
        }
    }
    if log.next_cursor.is_some() {
        out.line("")?;
        out.line_toned(
            &format!(
                "The {} most recent changes; older ones are in the web app.",
                log.entries.len()
            ),
            Tone::Dim,
        )?;
    }
    Ok(())
}
