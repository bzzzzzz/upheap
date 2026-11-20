//! `upheap parameter archive <parameter>`: archive a parameter no open item has a value for.

use api_types::parameters::{ParameterDto, ParameterListDto, TeamParameterDto, TeamParameterListDto};
use clap::Args;

use super::find_own_parameter;
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::config::SettingError;
use crate::lookup::{find_archived_parameter, find_archived_team_parameter, find_team_parameter};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ArchiveArgs {
    /// The parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    /// Find the parameter among this team's instead of a heap's: the team's slug
    #[arg(long, value_name = "TEAM", conflicts_with = "name")]
    pub team: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Archives the parameter. One that is archived already is archived again, since archiving says what the parameter
/// should end up as and the server answers with it unchanged; the output says so. The server refuses while an open
/// item has a value for it. Human output says what was done; `--quiet` prints nothing and `--json` the archived
/// parameter.
pub async fn run(args: ArchiveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let heap_name = match &args.team {
        Some(_) => None,
        None => Some(named_heap(args.heap.name.as_deref(), app)?),
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    if let Some(team) = &args.team {
        let api = TeamApi {
            client: &client,
            token: &token,
            team,
        };
        let list = api.get::<TeamParameterListDto>(&["parameters"]).await?;
        let (parameter, was_archived) = match find_team_parameter(&args.parameter, &list.value, team) {
            Ok(parameter) => (parameter, false),
            Err(err) => match find_archived_team_parameter(&args.parameter, &list.value) {
                Some(parameter) => (parameter, true),
                None => return Err(err.into()),
            },
        };
        let id = parameter.id.to_string();
        let archived = api
            .post::<TeamParameterDto>(&["parameters", &id, "archive"], &serde_json::json!({}))
            .await?;
        match out.mode() {
            Mode::Json => out.json(&archived.body)?,
            Mode::Quiet => {}
            Mode::Human if was_archived => out.line(&archived_line(&archived.value.name, true))?,
            Mode::Human => out.line(&format!(
                "Archived team parameter \"{}\" in {team}; the heaps that used it have it as archived.",
                archived.value.name
            ))?,
        }
        return Ok(());
    }
    let heap = resolve_heap_handle(heap_name.ok_or(SettingError::NoHeap)?, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let list = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
    // A parameter in use first, then an archived one, which the server archives again without change.
    let (parameter, was_archived) = match find_own_parameter(&args.parameter, &list.value, &heap) {
        Ok(parameter) => (parameter, false),
        Err(err) => match find_archived_parameter(&args.parameter, &list.value) {
            Some(parameter) => (parameter, true),
            None => return Err(err),
        },
    };
    let id = parameter.id.to_string();
    let archived = api
        .post::<ParameterDto>(&["parameters", &id, "archive"], &serde_json::json!({}))
        .await?;
    match out.mode() {
        Mode::Json => out.json(&archived.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&archived_line(&archived.value.name, was_archived))?,
    }
    Ok(())
}

/// What the archive did, from the parameter's name and whether the list had it archived already.
fn archived_line(name: &str, was_archived: bool) -> String {
    if was_archived {
        format!("Parameter \"{name}\" is archived already.")
    } else {
        format!("Archived parameter \"{name}\".")
    }
}
