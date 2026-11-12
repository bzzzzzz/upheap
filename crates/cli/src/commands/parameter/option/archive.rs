//! `upheap parameter option archive <parameter> <option> --replace-with <option>`: archive an option, moving the
//! open items that have it to another.

use api_types::parameters::{ArchiveOptionDto, ParameterDto, ParameterListDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::lookup::{find_archived_option, find_option, find_parameter};
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

/// Archives the option; one archived already is archived again, which the server answers without change, and the
/// output says so. Both options are looked up first, and naming one for both is a usage error with nothing
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
    // An option in use first, then an archived one, which the server archives again without change.
    let (option_id, label, was_archived) = match find_option(&args.option, parameter, &heap) {
        Ok(option) => (option.id, option.label.as_str(), false),
        Err(err) => match find_archived_option(&args.option, parameter) {
            Some(option) => (option.id, option.label.as_str(), true),
            None => return Err(err.into()),
        },
    };
    let replacement = find_option(&args.replace_with, parameter, &heap)?;
    if replacement.id == option_id {
        return Err(UsageError::ReplaceWithItself {
            option: label.to_owned(),
            parameter: parameter.name.clone(),
        }
        .into());
    }
    let id = option_id.to_string();
    let body = ArchiveOptionDto {
        replacement_option_id: Some(replacement.id),
    };
    let archived = api.post::<ParameterDto>(&["options", &id, "archive"], &body).await?;
    match out.mode() {
        Mode::Json => out.json(&archived.body)?,
        Mode::Quiet => {}
        Mode::Human if was_archived => {
            out.line(&format!(
                "Option \"{label}\" in \"{}\" is archived already.",
                archived.value.name
            ))?;
        }
        Mode::Human => out.line(&format!(
            "Archived option \"{label}\" in \"{}\"; open items with it now have \"{}\".",
            archived.value.name, replacement.label
        ))?,
    }
    Ok(())
}
