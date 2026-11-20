//! `upheap parameter option edit <parameter> <option> --label <label>`: relabel an option.

use api_types::parameters::{OptionDto, ParameterDto, ParameterListDto, TeamParameterDto, TeamParameterListDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, resolve_heap_handle};
use crate::commands::parameter::arrangement;
use crate::commands::parameter::find_own_parameter;
use crate::commands::{HeapArgs, SharedArgs};
use crate::config::SettingError;
use crate::lookup::{find_option, find_team_option, find_team_parameter};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    /// The option: its label, ignoring case, or its ID
    #[arg(id = "option_name", value_name = "OPTION")]
    pub option: String,
    /// The new label, 1 to 50 characters, not used by another option of the parameter ignoring case
    #[arg(long, required = true, value_name = "LABEL")]
    pub label: String,
    /// Find the parameter among this team's instead of a heap's: the team's slug
    #[arg(long, value_name = "TEAM", conflicts_with = "name")]
    pub team: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Relabels the option, sending every option of the parameter in its current order with the new label on this one,
/// since the API changes labels through the arrangement. Human output says whether the label changed; `--quiet`
/// prints nothing and `--json` the parameter.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
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
        let parameter = find_team_parameter(&args.parameter, &list.value, team)?;
        let option = find_team_option(&args.option, parameter, team)?;
        let id = parameter.id.to_string();
        let current: Vec<&OptionDto> = parameter.options.iter().collect();
        let body = arrangement(&current, Some((option.id, &args.label)));
        let edited = api
            .patch::<TeamParameterDto>(&["parameters", &id, "options"], parameter.version, &body)
            .await?;
        match out.mode() {
            Mode::Json => out.json(&edited.body)?,
            Mode::Quiet => {}
            Mode::Human if edited.value.version == parameter.version => out.line(&format!(
                "No change to option \"{}\" in team parameter \"{}\".",
                option.label, edited.value.name
            ))?,
            Mode::Human => {
                let kept = edited
                    .value
                    .options
                    .iter()
                    .find(|after| after.id == option.id)
                    .map_or(args.label.as_str(), |after| after.label.as_str());
                out.line(&format!(
                    "Renamed option \"{}\" to \"{kept}\" in team parameter \"{}\" in every heap of {team}.",
                    option.label, edited.value.name
                ))?;
            }
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
    let parameter = find_own_parameter(&args.parameter, &list.value, &heap)?;
    let option = find_option(&args.option, parameter, &heap)?;
    let id = parameter.id.to_string();
    let current: Vec<&OptionDto> = parameter.options.iter().collect();
    let body = arrangement(&current, Some((option.id, &args.label)));
    let edited = api
        .patch::<ParameterDto>(&["parameters", &id, "options"], parameter.version, &body)
        .await?;
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let kept = edited
                .value
                .options
                .iter()
                .find(|after| after.id == option.id)
                .map_or(args.label.as_str(), |after| after.label.as_str());
            out.line(&edit_line(option, kept, parameter, &edited.value))?;
        }
    }
    Ok(())
}

/// What the edit did, from the option as read, its label as the server kept it, and the parameter as read and as
/// returned.
fn edit_line(option: &OptionDto, kept: &str, before: &ParameterDto, after: &ParameterDto) -> String {
    if after.version == before.version {
        format!("No change to option \"{}\" in \"{}\".", option.label, after.name)
    } else {
        format!(
            "Renamed option \"{}\" to \"{kept}\" in \"{}\".",
            option.label, after.name
        )
    }
}
