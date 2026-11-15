//! `upheap work log [--cursor <cursor>]`: what the agents reached in the last 30 days, newest first.

use api_types::agents::{ResultsDto, ResultsQuery};
use clap::Args;

use crate::commands::SharedArgs;
use crate::commands::heap_api::indented_context;
use crate::output::{Cell, Mode, Output, Tone, shorten};
use crate::{App, CliError};

const TITLE_WIDTH: usize = 60;

#[derive(Args, Debug)]
pub struct LogArgs {
    /// Where the page starts: the `next_cursor` of the page before, for older runs
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints one page of the runs that reached their goal, under a heading for each day:
/// `14:02  k3x9a  Rotate the certs  Work  defined (skipped with context)`, then the run's result and its links, one
/// to a line. A page that isn't the last ends with the command for the older runs. No runs print a sentence saying
/// so. `--quiet` prints each run's item as `team/heap/key`; `--json` the server's answer. Exit 0 either way.
pub async fn run(args: LogArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let older = args.cursor.is_some();
    let query = ResultsQuery { cursor: args.cursor };
    let results = client.get::<ResultsDto>(&["me", "results"], &query, &token).await?;
    match out.mode() {
        Mode::Json => out.json(&results.body)?,
        Mode::Quiet => {
            for row in &results.value.rows {
                // The log spans heaps, so a key alone may name an item in another heap than the default.
                out.line(&format!("{}/{}/{}", row.heap.team_slug, row.heap.slug, row.item.key))?;
            }
        }
        Mode::Human => render(&results.value, older, out)?,
    }
    Ok(())
}

/// `older` is a page after the first: when it is empty, the runs before it have aged out of the 30 days.
fn render(results: &ResultsDto, older: bool, out: &mut Output) -> std::io::Result<()> {
    if results.rows.is_empty() {
        return out.line(if older {
            "No older runs in the last 30 days."
        } else {
            "No agent run reached anything in the last 30 days."
        });
    }
    let rows: Vec<Vec<Cell>> = results
        .rows
        .iter()
        .map(|row| {
            vec![
                Cell::toned(row.time.clone(), Tone::Dim),
                Cell::toned(row.item.key.clone(), Tone::Strong),
                Cell::new(shorten(&row.item.title, TITLE_WIDTH)),
                Cell::new(row.heap.name.clone()),
                Cell::new(row.reached_label.clone().unwrap_or_default()),
            ]
        })
        .collect();
    let lines = out.render_table(&rows);
    let mut day = None;
    for (row, line) in results.rows.iter().zip(lines) {
        if day != Some(&row.day) {
            day = Some(&row.day);
            out.line_toned(&upper_first(&row.day_label), Tone::Strong)?;
        }
        out.table_line("  ", &line)?;
        for text in indented_context(&row.result) {
            out.line(&format!("  {text}"))?;
        }
        for link in &row.links {
            if link.pull_request {
                out.line_parts(&[
                    ("    ", Tone::Plain),
                    (&link.label, Tone::Dim),
                    (" ", Tone::Plain),
                    (&link.url, Tone::Plain),
                ])?;
            } else {
                out.line(&format!("    {}", link.url))?;
            }
        }
    }
    if let Some(cursor) = &results.next_cursor {
        out.line("")?;
        out.line_toned(&format!("Older runs: upheap work log --cursor {cursor}"), Tone::Dim)?;
    }
    Ok(())
}

/// `text` with its first letter in upper case: "today" becomes "Today".
fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::output::human_text;

    fn row(day: &str, label: &str, time: &str, key: &str, title: &str, heap: &str) -> serde_json::Value {
        json!({
            "run_id": "00000000-0000-0000-0000-000000000000",
            "ended_at": 0,
            "day": day,
            "day_label": label,
            "time": time,
            "item": { "id": "00000000-0000-0000-0000-000000000000", "key": key, "title": title, "kind": "task",
                      "stage": "defined" },
            "heap": { "id": "00000000-0000-0000-0000-000000000000", "slug": "h", "name": heap, "team_slug": "ada",
                      "team_name": "Ada" },
            "goal": "with_context",
            "reached": "defined",
            "reached_label": "with context",
            "result": "",
            "links": [],
        })
    }

    #[gtest]
    fn rows_on_two_days_print_under_their_headings_with_results_and_links() {
        let mut first = row("2026-11-14", "today", "14:02", "k3x9a", "Rotate the certs", "Work");
        first["reached_label"] = json!("defined (skipped with context)");
        first["result"] = json!("Wrote the context.\nAnd the steps.");
        first["links"] = json!([
            { "url": "https://github.com/acme/app/pull/7", "label": "pull request", "pull_request": true },
            { "url": "https://wiki.example/page", "label": "wiki.example/page", "pull_request": false },
        ]);
        let mut second = row(
            "2026-11-13",
            "yesterday",
            "09:15",
            "m2p8q",
            "Fix the login bug",
            "House",
        );
        second["result"] = json!("Found the cause.");
        let results: ResultsDto =
            serde_json::from_value(json!({ "rows": [first, second], "next_cursor": "c1" })).expect("a results page");
        let text = human_text(|out| render(&results, false, out));

        expect_that!(
            text,
            eq(concat!(
                "Today\n",
                "  14:02  k3x9a  Rotate the certs   Work   defined (skipped with context)\n",
                "    Wrote the context.\n",
                "    And the steps.\n",
                "    pull request https://github.com/acme/app/pull/7\n",
                "    https://wiki.example/page\n",
                "Yesterday\n",
                "  09:15  m2p8q  Fix the login bug  House  with context\n",
                "    Found the cause.\n",
                "\n",
                "Older runs: upheap work log --cursor c1\n",
            ))
        );
    }
}
