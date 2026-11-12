//! `upheap parameter archive <parameter>`: archive a parameter no open item has a value for.

use api_types::parameters::{ParameterDto, ParameterListDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::{find_archived_parameter, find_parameter};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ArchiveArgs {
    /// The parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Archives the parameter. One that is archived already is said so and left alone, since archiving says what the
/// parameter should end up as. The server refuses while an open item has a value for it. Human output says what was
/// done; `--quiet` prints nothing and `--json` the archived parameter.
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
    let parameter = match find_parameter(&args.parameter, &list.value, &heap) {
        Ok(parameter) => parameter,
        Err(err) => {
            let Some(archived) = find_archived_parameter(&args.parameter, &list.value) else {
                return Err(err.into());
            };
            return report_archived_already(archived, out);
        }
    };
    let id = parameter.id.to_string();
    let archived = api
        .post::<ParameterDto>(&["parameters", &id, "archive"], &serde_json::json!({}))
        .await?;
    match out.mode() {
        Mode::Json => out.json(&archived.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!("Archived parameter \"{}\".", archived.value.name))?,
    }
    Ok(())
}

/// Says that `parameter` was archived before, and writes nothing.
fn report_archived_already(parameter: &ParameterDto, out: &mut Output) -> Result<(), CliError> {
    match out.mode() {
        Mode::Json => out.json(&serde_json::to_string(parameter).map_err(std::io::Error::from)?)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!("Parameter \"{}\" is archived already.", parameter.name))?,
    }
    Ok(())
}
