//! `upheap item search <text>`: items by title or key in every heap you can use, open and closed.

use api_types::items::{SearchQuery, SearchResultsDto};
use clap::Args;

use crate::commands::SharedArgs;
use crate::output::{Cell, Mode, Output, Tone, shorten};
use crate::{App, CliError};

use super::common::{TITLE_WIDTH, heap_label, state_text};

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Part of the title, ignoring case, or the whole key
    #[arg(value_name = "TEXT")]
    pub text: String,
    #[command(flatten)]
    pub shared: SharedArgs,
}

pub async fn run(args: SearchArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let query = SearchQuery { q: args.text.clone() };
    let results = client
        .get::<SearchResultsDto>(&["me", "search"], &query, &token)
        .await?;
    let rows = &results.value.rows;
    let names: Vec<String> = rows
        .iter()
        .map(|row| format!("{}/{}", heap_label(&row.heap), row.item.key))
        .collect();
    match out.mode() {
        Mode::Json => out.json(&results.body)?,
        Mode::Quiet => {
            for name in &names {
                out.line(name)?;
            }
        }
        Mode::Human => {
            if rows.is_empty() {
                out.line(&format!("No items in your heaps match \"{}\".", args.text))?;
                return Ok(());
            }
            let cells: Vec<Vec<Cell>> = rows
                .iter()
                .zip(&names)
                .map(|(row, name)| {
                    vec![
                        Cell::toned(name.clone(), Tone::Strong),
                        Cell::new(shorten(&row.item.title, TITLE_WIDTH)),
                        Cell::new(state_text(row.item.state)),
                    ]
                })
                .collect();
            for line in out.render_table(&cells) {
                out.table_line("", &line)?;
            }
            if results.value.truncated {
                out.line("")?;
                out.line_toned(
                    &format!(
                        "Showing the first {} matches; give more of the title to narrow it.",
                        rows.len()
                    ),
                    Tone::Dim,
                )?;
            }
        }
    }
    Ok(())
}
