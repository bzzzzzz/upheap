//! The parameter commands: their args and flows, and what the ones that write share.

pub mod add;
pub mod archive;
pub mod edit;
pub mod list;
pub mod option;
pub mod promote;
pub mod stop_using;
pub mod use_parameter;

use api_types::parameters::{
    ArrangeOptionsDto, OptionArrangementDto, OptionDto, ParameterDto, ParameterKindDto, ParameterListDto,
    TeamParameterListDto,
};
use clap::builder::RangedI64ValueParser;
use domain::parameter::Weight;
use uuid::Uuid;

use crate::client::Fetched;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::lookup::{HeapHandle, find_archived_team_parameter, find_parameter, find_team_parameter};
use crate::{App, CliError};

/// The parameter in use that `name` names, which the heap's own commands may change: a team parameter in the heap
/// is changed on the team, so naming one is a usage error that says how.
fn find_own_parameter<'p>(
    name: &str,
    list: &'p ParameterListDto,
    heap: &HeapHandle,
) -> Result<&'p ParameterDto, CliError> {
    let parameter = find_parameter(name, list, heap)?;
    if parameter.kind == ParameterKindDto::Team {
        return Err(UsageError::TeamParameterInHeap {
            name: parameter.name.clone(),
            team: heap.team.clone(),
        }
        .into());
    }
    Ok(parameter)
}

/// The parser of `--weight`: 0 to 100, so a weight outside it is a usage error before any request.
fn weight_parser() -> RangedI64ValueParser<u8> {
    clap::value_parser!(u8).range(..=i64::from(Weight::MAX))
}

/// The help of `--weight`, which says what a new parameter gets when it is left out.
fn describe_weight_flag() -> String {
    format!(
        "How much it counts in the ranking, 0 to {}. Left out, {}",
        Weight::MAX,
        Weight::DEFAULT.get()
    )
}

/// An arrangement of `options`, in the order given and with their current labels, except that `relabel` gives one
/// option, by ID, a new label.
fn arrangement(options: &[&OptionDto], relabel: Option<(Uuid, &str)>) -> ArrangeOptionsDto {
    ArrangeOptionsDto {
        options: options
            .iter()
            .map(|option| OptionArrangementDto {
                id: option.id,
                label: match relabel {
                    Some((id, label)) if id == option.id => label.to_owned(),
                    _ => option.label.clone(),
                },
            })
            .collect(),
    }
}

/// What a heap's use of a team parameter came to: the parameter, the heap, and the heap's list as the server
/// returned it.
struct Used {
    name: String,
    heap: HeapHandle,
    list: Fetched<ParameterListDto>,
}

/// Looks the team parameter `typed` up among its team's, in use or archived, and sends the heap's `action`
/// (`use` or `stop-using`) for it. The team is the heap's.
async fn change_use(
    action: &str,
    typed: &str,
    heap: &HeapArgs,
    shared: &SharedArgs,
    app: &App,
) -> Result<Used, CliError> {
    let name = named_heap(heap.name.as_deref(), app)?;
    let client = app.client(app.server(shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let team = TeamApi {
        client: &client,
        token: &token,
        team: &heap.team,
    };
    let team_list = team.get::<TeamParameterListDto>(&["parameters"]).await?;
    let parameter = match find_team_parameter(typed, &team_list.value, &heap.team) {
        Ok(parameter) => parameter,
        Err(err) => find_archived_team_parameter(typed, &team_list.value).ok_or(err)?,
    };
    let id = parameter.id.to_string();
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let list = api
        .post::<ParameterListDto>(&["parameters", &id, action], &serde_json::json!({}))
        .await?;
    Ok(Used {
        name: parameter.name.clone(),
        heap,
        list,
    })
}
