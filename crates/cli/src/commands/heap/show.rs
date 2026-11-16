//! `upheap heap show [<heap>]`: a heap's name, your priority for it, its ranking weights and its context.

use api_types::heaps::{HeapDto, HeapWeightsDto};
use api_types::me::MeDto;
use api_types::parameters::{ParameterDto, ParameterListDto};
use api_types::tags::TagListDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::commands::heap_api::{HeapApi, indented_context, named_heap};
use crate::lookup::resolve_heap;
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// team/heap, or a heap slug only one of your teams has. Without it, the default heap
    #[arg(value_name = "HEAP")]
    pub heap: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints the heap's name, its team, its ranking weights and its context. The parameters' weights cost a request,
/// made only for human output. `--json` prints the heap as the API returns it, and `--quiet` nothing.
pub async fn run(args: ShowArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
    let handle = resolve_heap(name, &me.value)?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &handle,
    };
    let heap = api.get::<HeapDto>(&[], NO_QUERY).await?;
    match out.mode() {
        Mode::Json => out.json(&heap.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let team = me
                .value
                .teams
                .iter()
                .find(|team| team.slug == handle.team)
                .map_or(handle.team.as_str(), |team| team.name.as_str());
            let parameters = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
            let tag_order = tag_order_names(&heap.value, &api).await?;
            render(
                &heap.value,
                &tag_order,
                &parameters.value.parameters,
                team,
                &handle.to_string(),
                out,
            )?;
        }
    }
    Ok(())
}

/// The names of the heap's ranked tags, the highest first. The tags cost a request, made only when the heap has an
/// order.
pub(super) async fn tag_order_names(heap: &HeapDto, api: &HeapApi<'_>) -> Result<Vec<String>, CliError> {
    if heap.tag_order.is_empty() {
        return Ok(Vec::new());
    }
    let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
    Ok(heap
        .tag_order
        .iter()
        .map(|id| {
            tags.value
                .tags
                .iter()
                .find(|tag| tag.id == *id)
                .map_or_else(|| id.to_string(), |tag| tag.name.clone())
        })
        .collect())
}

fn render(
    heap: &HeapDto,
    tag_order: &[String],
    parameters: &[ParameterDto],
    team: &str,
    handle: &str,
    out: &mut Output,
) -> std::io::Result<()> {
    let archived = if heap.archived_at.is_some() { " · archived" } else { "" };
    let prepare = if heap.agents_prepare { "on" } else { "off" };
    out.line_toned(
        &format!(
            "{} · {team}{archived} · agents prepare new notes: {prepare} · your priority: {}",
            heap.name, heap.priority
        ),
        Tone::Strong,
    )?;
    out.line("")?;
    render_weights(&heap.weights, tag_order, parameters, out)?;
    out.line("")?;
    out.line_toned("Context", Tone::Strong)?;
    match heap.context.as_deref().filter(|text| !text.is_empty()) {
        Some(context) => {
            for line in indented_context(context) {
                out.line(&line)?;
            }
        }
        None => out.line_toned(
            &format!("  None. Write one with: upheap heap edit {handle} --context-file <file>"),
            Tone::Dim,
        )?,
    }
    Ok(())
}

/// The Weights section: the heap's five weights, then each of `parameters` (those in use) in the order given, with
/// the word `parameter` dim after it, then the names of the ranked tags in `tag_order` when there are any. Doesn't
/// end with a blank line.
pub(super) fn render_weights(
    weights: &HeapWeightsDto,
    tag_order: &[String],
    parameters: &[ParameterDto],
    out: &mut Output,
) -> std::io::Result<()> {
    let mut rows: Vec<Vec<Cell>> = [
        ("Need", weights.need),
        ("Age", weights.age),
        ("Deadline", weights.deadline),
        ("Value", weights.value),
        ("Tags", weights.tags),
    ]
    .into_iter()
    .map(|(name, weight)| vec![Cell::new(name), Cell::new(weight_text(weight))])
    .collect();
    rows.extend(parameters.iter().map(|parameter| {
        vec![
            Cell::new(parameter.name.clone()),
            Cell::new(weight_text(parameter.weight)),
            Cell::toned("parameter", Tone::Dim),
        ]
    }));
    out.line_toned("Weights", Tone::Strong)?;
    for line in out.render_table(&rows) {
        out.table_line("  ", &line)?;
    }
    if !tag_order.is_empty() {
        out.line(&format!("Tag order: {}", tag_order.join(", ")))?;
    }
    Ok(())
}

/// A weight as printed: its number, and `(off)` after a zero, which turns the factor off.
fn weight_text(weight: u16) -> String {
    if weight == 0 {
        "0 (off)".to_owned()
    } else {
        weight.to_string()
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn a_zero_weight_reads_off_and_any_other_is_its_number() {
        expect_that!(weight_text(0), eq("0 (off)"));
        expect_that!(weight_text(1), eq("1"));
        expect_that!(weight_text(100), eq("100"));
    }
}
