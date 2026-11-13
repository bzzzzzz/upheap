//! `upheap item show <item>`: one item in full, and with `--history` the changes made to it.

use api_types::audit::{AuditLogDto, AuditQueryDto};
use api_types::common::{
    ItemDto, KindDto, MadeItemDto, NeedDto, QuestionDto, QuestionKindDto, QuestionStateDto, StageDto, StepStateDto,
};
use api_types::turns::{GoalDto, ReachDto, TurnDto, TurnSourceDto};
use clap::Args;

use super::common::{locate_item, state_text};
use crate::commands::comment::comment_lines;
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
            render_item(&item.value, app.env.now, out)?;
            if let Some(history) = history {
                render_history(&history.value, out)?;
            }
        }
    }
    Ok(())
}

/// What `item show` prints for `item` in human mode, without colour: the text an agent's brief carries.
pub(crate) fn item_text(item: &ItemDto, now: jiff::Timestamp) -> String {
    /// A writer that keeps what it is given, to read it back as text.
    #[derive(Clone, Default)]
    struct TextBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for TextBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let buffer = TextBuffer::default();
    let mut out = Output::new(Box::new(buffer.clone()), Box::new(std::io::sink()), Mode::Human, false);
    // The buffer takes every write, so rendering cannot fail.
    let _ = render_item(item, now, &mut out);
    let bytes = buffer.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn render_item(item: &ItemDto, now: jiff::Timestamp, out: &mut Output) -> std::io::Result<()> {
    let head = [vec![
        Cell::toned(item.key.clone(), Tone::Strong),
        Cell::new(item.title.clone()),
    ]];
    for line in out.render_table(&head) {
        out.table_line("", &line)?;
    }
    let lines = out.render_table(&fields(item, now));
    for line in &lines {
        out.table_line("", line)?;
    }
    if let Some(turn) = &item.turn {
        out.line(&turn_text(turn))?;
        if let Some(reason) = turn
            .stuck_reason
            .as_deref()
            .filter(|reason| turn.stuck && !reason.is_empty())
        {
            out.line(&format!("  {}", reason.replace('\n', " ")))?;
        }
    }
    if let Some(text) = from_text(item) {
        out.line(&text)?;
    }
    if let Some(became) = item.became.as_deref().filter(|became| !became.is_empty()) {
        out.line("Became:")?;
        for made in became {
            out.line(&format!("  {}", made_item_text(made)))?;
        }
    }
    if let Some(description) = item.description.as_deref().filter(|text| !text.is_empty()) {
        out.line("")?;
        out.line_toned("Context", Tone::Strong)?;
        for line in indented_context(description) {
            out.line(&line)?;
        }
    }
    render_comments(item, out)?;
    render_steps(item, out)?;
    render_questions(item, out)
}

/// An item a note became as one phrase: `n8c2d "Rotate staging" (task, open)`.
fn made_item_text(item: &MadeItemDto) -> String {
    format!(
        "{} \"{}\" ({}, {})",
        item.key,
        item.title,
        kind_text(item.kind),
        state_text(item.state)
    )
}

/// The item's comments, when it has any: a heading and then the thread as `comment list` prints it.
fn render_comments(item: &ItemDto, out: &mut Output) -> std::io::Result<()> {
    let lines = comment_lines(item);
    if lines.is_empty() {
        return Ok(());
    }
    out.line("")?;
    out.line_toned("Comments", Tone::Strong)?;
    for line in lines {
        out.line(&line)?;
    }
    Ok(())
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

/// The item's questions and requests, when it has any, in number order: a line with the question's name, kind, state
/// and step, then its text, details, choices and reply, aligned under the name.
fn render_questions(item: &ItemDto, out: &mut Output) -> std::io::Result<()> {
    // `null` (a list row) reads as no questions; `item show` reads a single item, which always has the array.
    let questions = item.questions.as_deref().unwrap_or_default();
    if questions.is_empty() {
        return Ok(());
    }
    out.line("")?;
    out.line_toned("Questions", Tone::Strong)?;
    let names: Vec<String> = questions
        .iter()
        .map(|question| format!("{}.q{}", item.key, question.number))
        .collect();
    let width = names.iter().map(|name| name.chars().count()).max().unwrap_or(0);
    let indent = " ".repeat(2 + width + 2);
    for (question, name) in questions.iter().zip(&names) {
        let mut head = format!("{} · {}", question_kind_text(question), question_state_text(question));
        if let Some(step) = question.step {
            head.push_str(&format!(" · step {}.{step}", item.key));
        }
        out.line(&format!("  {name:<width$}  {head}"))?;
        for line in question_lines(question) {
            out.line(&format!("{indent}{line}"))?;
        }
    }
    Ok(())
}

/// What goes under a question's head line, each line relative to the column the text starts at.
fn question_lines(question: &QuestionDto) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(review) = &question.review {
        lines.push(review.link.clone());
        if review.piece != review.link {
            lines.push(format!("Piece: {}", review.piece.replace('\t', "    ")));
        }
    }
    lines.push(question.text.replace('\t', "    "));
    for line in question.details.iter().flat_map(|details| details.lines()) {
        lines.push(format!("  {}", line.replace('\t', "    ")));
    }
    // A request, a review and a done proposal always have the two choices of their kind, which the reader knows.
    let fixed = matches!(
        question.kind,
        QuestionKindDto::Request | QuestionKindDto::Review | QuestionKindDto::DoneProposal
    );
    if !fixed && !question.choices.is_empty() {
        let how = if question.many { "pick any" } else { "pick one" };
        lines.push(format!("Choices ({how}): {}", question.choices.join(" · ")));
    }
    if let Some(reply) = &question.reply {
        let picked: Vec<&str> = reply
            .picked
            .iter()
            .filter_map(|index| question.choices.get(usize::try_from(*index).ok()?))
            .map(String::as_str)
            .collect();
        let mut arrow = String::from("→");
        if !picked.is_empty() {
            arrow.push_str(&format!(" {}", picked.join(", ")));
        }
        let comment_lines: Vec<String> = reply
            .comment
            .iter()
            .flat_map(|comment| comment.lines())
            .map(|line| line.replace('\t', "    "))
            .collect();
        match comment_lines.as_slice() {
            [] => lines.push(arrow),
            [only] => lines.push(format!("{arrow} \"{only}\"")),
            many => {
                lines.push(arrow);
                lines.extend(many.iter().map(|line| format!("  {line}")));
            }
        }
    }
    lines
}

/// The kind as the head line reads it; a review also says its round.
fn question_kind_text(question: &QuestionDto) -> String {
    match (question.kind, &question.review) {
        (QuestionKindDto::Question, _) => "Question".to_owned(),
        (QuestionKindDto::Request, _) => "Request".to_owned(),
        (QuestionKindDto::Review, Some(review)) => format!("Review, round {}", review.round),
        (QuestionKindDto::Review, None) => "Review".to_owned(),
        (QuestionKindDto::DoneProposal, _) => "Done proposal".to_owned(),
        (QuestionKindDto::Unknown, _) => "Other".to_owned(),
    }
}

/// The state as the head line reads it. An answered review says whether it was approved, and an answered done
/// proposal whether it was agreed to, by the pick: the first choice agrees.
fn question_state_text(question: &QuestionDto) -> &'static str {
    let agreed = question.reply.as_ref().map(|reply| reply.picked.first() == Some(&0));
    match (question.state, question.kind, agreed) {
        (QuestionStateDto::Answered, QuestionKindDto::Review, Some(true)) => "approved",
        (QuestionStateDto::Answered, QuestionKindDto::Review, Some(false)) => "changes asked",
        (QuestionStateDto::Answered, QuestionKindDto::DoneProposal, Some(true)) => "agreed",
        (QuestionStateDto::Answered, QuestionKindDto::DoneProposal, Some(false)) => "not yet",
        (QuestionStateDto::Open, ..) => "open",
        (QuestionStateDto::Answered, ..) => "answered",
        (QuestionStateDto::Withdrawn, ..) => "withdrawn",
        (QuestionStateDto::Unknown, ..) => "unknown",
    }
}

/// The item's labelled fields, as rows of label and value. Unset sizes, due time, tags and the rest are left out.
fn fields(item: &ItemDto, now: jiff::Timestamp) -> Vec<Vec<Cell>> {
    let mut rows = vec![vec![Cell::new("Kind"), Cell::new(kind_text(item.kind))]];
    if let Some(stage) = item.stage {
        rows.push(vec![Cell::new("Stage"), Cell::new(stage_text(stage))]);
    }
    if let Some(due) = item.due.as_ref().filter(|_| super::add::is_to_come(item, now)) {
        rows.push(vec![Cell::new("Joins list"), Cell::new(due.date.clone())]);
    }
    let state = if item.with_agent {
        format!("{} · with an agent", state_text(item.state))
    } else {
        state_text(item.state).to_owned()
    };
    rows.push(vec![Cell::new("State"), Cell::new(state)]);
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
        // A blocker moved away is named with the heap it is in now, where its key is.
        let moved = item
            .blocker_gone
            .as_ref()
            .and_then(|gone| gone.moved_to.as_ref())
            .map(|heap| format!(", in {}", heap.name))
            .unwrap_or_default();
        rows.push(vec![
            Cell::new("Blocked by"),
            Cell::new(format!(
                "{}  {} ({}{moved})",
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

/// Where the item came from, when it has an origin or an agent run added it. `From: k3x9a "Rotate the certs" (task,
/// acme/ops), added by an agent run`; `From: an agent run` when the origin isn't one the caller can see.
fn from_text(item: &ItemDto) -> Option<String> {
    match (&item.origin, item.added_in_run) {
        (Some(origin), added_in_run) => {
            let heap = &origin.heap;
            let mut text = format!(
                "From: {} \"{}\" ({}, {}/{})",
                origin.key,
                origin.title.replace('\n', " "),
                kind_text(origin.kind),
                heap.team_slug,
                heap.slug
            );
            if added_in_run {
                text.push_str(", added by an agent run");
            }
            Some(text)
        }
        (None, true) => Some("From: an agent run".to_owned()),
        (None, false) => None,
    }
}

/// The open turn as one line: where it goes, the reach when it is all the way, the person's line in quotes, and
/// whether the heap prepared it. `Turn: to defined, all the way. "Use the new CA." Prepared on add.`
fn turn_text(turn: &TurnDto) -> String {
    let mut text = format!("Turn: to {}", goal_text(turn.goal));
    if turn.reach == ReachDto::AllTheWay {
        text.push_str(", all the way");
    }
    text.push('.');
    if let Some(line) = turn.line.as_deref().filter(|line| !line.is_empty()) {
        text.push_str(&format!(" \"{}\".", line.replace('\n', " ")));
    }
    if turn.source == TurnSourceDto::Prepare {
        text.push_str(" Prepared on add.");
    }
    if turn.runs > 0 {
        text.push_str(&format!(" Runs: {}.", turn.runs));
    }
    if turn.stuck {
        text.push_str(&format!(" Stuck: {} runs in a row failed.", turn.failed_runs));
    }
    text
}

const fn goal_text(goal: GoalDto) -> &'static str {
    match goal {
        GoalDto::WithContext => "with context",
        GoalDto::Defined => "defined",
        GoalDto::Done => "done",
        GoalDto::Unknown => "unknown",
    }
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
        KindDto::Reminder => "reminder",
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
