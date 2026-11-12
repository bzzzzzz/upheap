//! `upheap parameter option archive <parameter> <option> --replace-with <option>`: archive an option, moving the
//! open items that have it to another.

use api_types::parameters::{ArchiveOptionDto, ParameterDto, ParameterListDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::lookup::{find_option, find_parameter};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ArchiveArgs {
    /// The parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    /// The option to archive: its label, ignoring case, or its ID
    #[arg(id = "option_name", value_name = "OPTION")]
    pub option: String,
    /// The option open items that have it get instead, another of the parameter's, by label or ID
    #[arg(long, required = true, value_name = "OPTION")]
    pub replace_with: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Archives the option. Both options are looked up first, and naming one for both is a usage error with nothing
/// sent. Human output names the option that took over; `--quiet` prints nothing and `--json` the parameter.
pub async fn run(args: ArchiveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
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
    let option = find_option(&args.option, parameter, &heap)?;
    let replacement = find_option(&args.replace_with, parameter, &heap)?;
    if replacement.id == option.id {
        return Err(UsageError::ReplaceWithItself {
            option: option.label.clone(),
            parameter: parameter.name.clone(),
        }
        .into());
    }
    let id = option.id.to_string();
    let body = ArchiveOptionDto {
        replacement_option_id: Some(replacement.id),
    };
    let archived = api.post::<ParameterDto>(&["options", &id, "archive"], &body).await?;
    match out.mode() {
        Mode::Json => out.json(&archived.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!(
            "Archived option \"{}\" in \"{}\"; open items with it now have \"{}\".",
            option.label, archived.value.name, replacement.label
        ))?,
    }
    Ok(())
}
