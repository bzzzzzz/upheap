//! `upheap question list`: the questions, requests, reviews and done proposals that wait for the signed-in user,
//! across their heaps or in one, most important first, each named so that `question reply` takes it as printed.
//!
//! One `GET /me/waiting`. Rows that carry no question (stuck turns, gone blockers, kinds this client doesn't know)
//! are left out. The default heap is not applied: the point is the view across heaps. `--json` prints the server's
//! body, so `--heap`, which filters it here, doesn't go with it.

use api_types::common::QuestionDto;
use api_types::waiting::{ItemHeapDto, WaitingDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::commands::heap_api::resolve_heap_handle;
use crate::commands::item::show::question_kind_text;
use crate::lookup::HeapName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only this heap's: team/heap, or a heap slug only one of your teams has. Without it, all your heaps
    // Its id is in the guide's list of `--heap` flags that apply no default heap.
    #[arg(id = "only_heap", long = "heap", value_name = "HEAP", conflicts_with = "json")]
    pub heap: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists what waits for the user. Human output is a few lines per question; `--quiet` prints the names and `--json`
/// the server's body.
pub async fn run_list(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let only = match &args.heap {
        Some(typed) => Some(resolve_heap_handle(HeapName::parse(typed)?, &client, &token).await?),
        None => None,
    };
    let waiting = client.get::<WaitingDto>(&["me", "waiting"], NO_QUERY, &token).await?;
    if out.mode() == Mode::Json {
        return Ok(out.json(&waiting.body)?);
    }
    let rows: Vec<_> = waiting
        .value
        .rows
        .iter()
        .filter_map(|row| Some((row, row.question.as_ref()?)))
        .filter(|(row, _)| {
            only.as_ref()
                .is_none_or(|only| row.heap.team_slug == only.team && row.heap.slug == only.heap)
        })
        .collect();
    if out.mode() == Mode::Quiet {
        for (row, question) in rows {
            out.line(&name(&row.heap, &row.item.key, question))?;
        }
        return Ok(());
    }
    if rows.is_empty() {
        out.line("Nothing waits for you.")?;
    }
    for (row, question) in rows {
        let title = row.item.title.replace('\t', "    ");
        out.line(&format!(
            "{}  {} · {title}",
            name(&row.heap, &row.item.key, question),
            question_kind_text(question)
        ))?;
        if let Some(review) = &question.review {
            out.line(&format!("  {}", review.link))?;
        }
        out.line(&format!("  {}", question.text.replace('\t', "    ")))?;
        out.line(&format!("  {}", how_to_reply(question)))?;
    }
    Ok(())
}

/// `team/heap/key.qN`, which `question reply` takes without `--heap`.
fn name(heap: &ItemHeapDto, key: &str, question: &QuestionDto) -> String {
    format!("{}/{}/{key}.q{}", heap.team_slug, heap.slug, question.number)
}

fn how_to_reply(question: &QuestionDto) -> String {
    if question.choices.is_empty() {
        return "Free text: reply with --comment".to_owned();
    }
    let choices: Vec<String> = question
        .choices
        .iter()
        .enumerate()
        .map(|(index, choice)| format!("{} {choice}", index + 1))
        .collect();
    let how = if question.many { "Pick any" } else { "Pick one" };
    format!("{how}: {}", choices.join(" · "))
}
