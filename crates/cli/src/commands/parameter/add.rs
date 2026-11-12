//! `upheap parameter add <name>`: add a parameter to a heap, with its options and the tags it is suggested for.

use api_types::parameters::{CreateParameterDto, ParameterDto};
use api_types::tags::TagListDto;
use clap::Args;

use super::{describe_weight_flag, weight_parser};
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::item::common::dedupe;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::{LookupError, resolve_tags};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The parameter's name, 1 to 50 characters, not used by another parameter in use ignoring case
    #[arg(id = "parameter_name", value_name = "NAME")]
    pub name: String,
    #[arg(long, value_name = "N", value_parser = weight_parser(), help = describe_weight_flag())]
    pub weight: Option<u8>,
    /// An option, 1 to 50 characters, highest priority first. Repeat for more
    #[arg(long, value_name = "LABEL")]
    pub option: Vec<String>,
    /// A tag of the heap it's suggested for, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub tag: Vec<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the parameter in one request, with the name and labels as typed: the server checks them. Tags are looked
/// up first, and only when `--tag` is given. Human output says how many options it has; `--quiet` prints the name
/// the server kept and `--json` the created parameter.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let tag_ids = if args.tag.is_empty() {
        Vec::new()
    } else {
        let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
        dedupe(resolve_tags(&args.tag, &tags.value, &heap).map_err(LookupError::offering_add)?)
    };
    let body = CreateParameterDto {
        name: args.name,
        weight: args.weight.map(i64::from),
        options: args.option,
        tag_ids,
    };
    let added = api.post::<ParameterDto>(&["parameters"], &body).await?;
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&added.value.name)?,
        Mode::Human => out.line(&added_line(&added.value))?,
    }
    Ok(())
}

/// What was added, from the parameter the server returned.
fn added_line(parameter: &ParameterDto) -> String {
    match parameter.options.len() {
        0 => format!("Added parameter \"{}\".", parameter.name),
        1 => format!("Added parameter \"{}\" with 1 option.", parameter.name),
        n => format!("Added parameter \"{}\" with {n} options.", parameter.name),
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn parameter_with(options: usize) -> googletest::Result<ParameterDto> {
        let options: Vec<_> = (0..options)
            .map(|n| json!({ "id": Uuid::from_u128(n as u128), "label": format!("Option {n}") }))
            .collect();
        Ok(serde_json::from_value(json!({
            "id": Uuid::new_v4(), "heap_id": Uuid::new_v4(), "name": "Area", "weight": 10, "options": options,
            "archived_options": [], "tag_ids": [], "archived_at": null, "version": 1, "allowed_actions": [],
        }))?)
    }

    #[gtest]
    fn the_added_line_counts_the_options() -> googletest::Result<()> {
        verify_that!(added_line(&parameter_with(0)?), eq("Added parameter \"Area\"."))?;
        verify_that!(
            added_line(&parameter_with(1)?),
            eq("Added parameter \"Area\" with 1 option.")
        )?;
        verify_that!(
            added_line(&parameter_with(2)?),
            eq("Added parameter \"Area\" with 2 options.")
        )
    }
}
