//! `upheap parameter option add <parameter> <label>`: add an option to a parameter as its lowest priority.

use api_types::parameters::{AddOptionDto, ParameterDto, ParameterListDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::find_parameter;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    /// The option's label, 1 to 50 characters, not used by another option of the parameter ignoring case
    #[arg(id = "option_label", value_name = "LABEL")]
    pub label: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the option with the label as typed: the server checks it. Human output names the option and the parameter;
/// `--quiet` prints the label the server kept and `--json` the parameter with its new option.
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
    let list = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
    let parameter = find_parameter(&args.parameter, &list.value, &heap)?;
    let id = parameter.id.to_string();
    let body = AddOptionDto { label: args.label };
    let added = api
        .post_at_version::<ParameterDto>(&["parameters", &id, "options"], parameter.version, &body)
        .await?;
    // The new option comes last; the label as typed stands in if the server's answer has none.
    let kept = added
        .value
        .options
        .last()
        .map_or(body.label.as_str(), |option| option.label.as_str());
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(kept)?,
        Mode::Human => out.line(&format!("Added option \"{kept}\" to \"{}\".", added.value.name))?,
    }
    Ok(())
}
