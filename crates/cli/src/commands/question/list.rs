//! `upheap question list`: the questions, requests, reviews and done proposals that wait for the signed-in user,
//! across their heaps or in one, most important first, each named so that `question reply` takes it as printed.
//!
//! One `GET /me/waiting`, and `GET /me` to check the heap `--heap` names, as the server can't filter by heap. Rows
//! that carry no question (stuck turns, gone blockers, kinds this client doesn't know) are left out, and a footer
//! counts them, since `question reply` can't act on them and the web app's Waiting for me page can. The default heap
//! is not applied: the point is the view across heaps. `--json` prints the server's body, so `--heap`, which filters
//! it here, doesn't go with it.

use api_types::common::QuestionDto;
use api_types::me::MeDto;
use api_types::waiting::{ItemHeapDto, WaitingDto, WaitingRowDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::commands::item::show::question_kind_text;
use crate::lookup::{HeapName, resolve_heap};
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
        Some(typed) => {
            let name = HeapName::parse(typed)?;
            // Checked against the caller's heaps even when typed in full: a mistyped heap would list nothing.
            let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
            Some(resolve_heap(name, &me.value)?)
        }
        None => None,
    };
    let waiting = client.get::<WaitingDto>(&["me", "waiting"], NO_QUERY, &token).await?;
    if out.mode() == Mode::Json {
        return Ok(out.json(&waiting.body)?);
    }
    let in_heap: Vec<&WaitingRowDto> = waiting
        .value
        .rows
        .iter()
        .filter(|row| {
            only.as_ref()
                .is_none_or(|only| row.heap.team_slug == only.team && row.heap.slug == only.heap)
        })
        .collect();
    let rows: Vec<_> = in_heap
        .iter()
        .filter_map(|row| Some((*row, row.question.as_ref()?)))
        .collect();
    let others = in_heap.len() - rows.len();
    if out.mode() == Mode::Quiet {
        for (row, question) in rows {
            out.line(&name(&row.heap, &row.item.key, question))?;
        }
        return Ok(());
    }
    if rows.is_empty() && others == 0 {
        out.line("Nothing waits for you.")?;
    } else if rows.is_empty() {
        out.line("No questions wait for you.")?;
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
    if others > 0 {
        out.line(&format!(
            "Also waiting, on the web app's Waiting for me page: {others} (stuck turns, gone blockers)."
        ))?;
    }
    let returned = waiting.value.rows.len();
    if usize::try_from(waiting.value.count).is_ok_and(|count| count > returned) {
        out.line(&format!(
            "The server sent the first {returned} of {} things that wait; the web app's Waiting for me page has the rest.",
            waiting.value.count
        ))?;
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
