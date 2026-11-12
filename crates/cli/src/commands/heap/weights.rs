//! `upheap heap weights [<heap>]`: change a heap's ranking weights, its parameters' included, in one save.

use std::collections::BTreeMap;

use api_types::heaps::{HeapDto, HeapWeightsPatchDto, HeapWeightsResultDto, ParameterWeightChangeDto};
use api_types::me::MeDto;
use api_types::parameters::ParameterListDto;
use clap::Args;
use uuid::Uuid;

use crate::client::NO_QUERY;
use crate::commands::heap::show::render_weights;
use crate::commands::heap_api::{HeapApi, named_heap};
use crate::commands::{SharedArgs, UsageError};
use crate::lookup::{HeapHandle, find_parameter, resolve_heap};
use crate::output::{Mode, Output};
use crate::{App, CliError};

/// The highest weight a factor can have.
const MAX_WEIGHT: u16 = 100;

#[derive(Args, Debug)]
pub struct WeightsArgs {
    /// team/heap, or a heap slug only one of your teams has. Without it, the default heap
    #[arg(value_name = "HEAP")]
    pub heap: Option<String>,
    /// The weight of need, 0 to 100
    #[arg(long, value_name = "N", value_parser = parse_weight)]
    pub need: Option<u16>,
    /// The weight of how long an item has waited, 0 to 100
    #[arg(long, value_name = "N", value_parser = parse_weight)]
    pub age: Option<u16>,
    /// The weight of the due date, 0 to 100
    #[arg(long, value_name = "N", value_parser = parse_weight)]
    pub deadline: Option<u16>,
    /// The weight of value per effort (impact and effort together), 0 to 100
    #[arg(long, value_name = "N", value_parser = parse_weight)]
    pub value: Option<u16>,
    /// A parameter's weight, as NAME=N, the parameter by name or ID, 0 to 100. Repeat for more
    #[arg(long = "parameter", value_name = "NAME=N", value_parser = parse_parameter_weight)]
    pub parameters: Vec<ParameterWeight>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// One `--parameter NAME=N`: the parameter as typed, and the weight to give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterWeight {
    pub parameter: String,
    pub weight: u16,
}

/// Reads a weight: a whole number from 0 to 100.
fn parse_weight(raw: &str) -> Result<u16, String> {
    match raw.trim().parse::<u16>() {
        Ok(weight) if weight <= MAX_WEIGHT => Ok(weight),
        _ => Err(format!("`{raw}` isn't a whole number from 0 to {MAX_WEIGHT}")),
    }
}

/// Reads `NAME=N`, split at the last `=` so a name may hold one (a weight can't). Both sides are trimmed, as the
/// server trims names, and the name may not be empty.
fn parse_parameter_weight(raw: &str) -> Result<ParameterWeight, String> {
    let bad = || format!("--parameter takes NAME=N, such as --parameter Area=20; got `{raw}`");
    let (parameter, weight) = raw.rsplit_once('=').ok_or_else(bad)?;
    let parameter = parameter.trim();
    let weight = parse_weight(weight).map_err(|_| bad())?;
    if parameter.is_empty() {
        return Err(bad());
    }
    Ok(ParameterWeight {
        parameter: parameter.to_owned(),
        weight,
    })
}

/// Sends one save with the weights given, and says whether anything changed. `--json` prints the server's body and
/// `--quiet` nothing. The server decides who may change weights.
pub async fn run(args: WeightsArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let nothing_given = args.need.is_none()
        && args.age.is_none()
        && args.deadline.is_none()
        && args.value.is_none()
        && args.parameters.is_empty();
    if nothing_given {
        if out.mode() == Mode::Human {
            out.line("Nothing to change: give --need, --age, --deadline, --value or --parameter.")?;
        }
        return Ok(());
    }
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
    let current = api.get::<HeapDto>(&[], NO_QUERY).await?;
    let changes = if args.parameters.is_empty() {
        Vec::new()
    } else {
        let list = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
        resolve_parameter_weights(&args.parameters, &list.value, &handle)?
    };
    let patch = HeapWeightsPatchDto {
        need: args.need.map(i64::from),
        age: args.age.map(i64::from),
        deadline: args.deadline.map(i64::from),
        value: args.value.map(i64::from),
        parameters: changes,
    };
    let saved = api
        .patch::<HeapWeightsResultDto>(&["weights"], current.value.version, &patch)
        .await?;
    match out.mode() {
        Mode::Json => out.json(&saved.body)?,
        Mode::Quiet => {}
        Mode::Human if saved.value.heap.version == current.value.version => {
            out.line(&format!("No change to {handle}."))?;
        }
        Mode::Human => {
            out.line(&format!("Updated the weights of {handle}."))?;
            out.line("")?;
            render_weights(&saved.value.heap.weights, &saved.value.parameters, out)?;
        }
    }
    Ok(())
}

/// The change for each parameter named, by ID with the version just fetched. A parameter named twice with the same
/// weight is kept once; with different weights it is a usage error.
fn resolve_parameter_weights(
    given: &[ParameterWeight],
    list: &ParameterListDto,
    heap: &HeapHandle,
) -> Result<Vec<ParameterWeightChangeDto>, CliError> {
    let mut changes: BTreeMap<Uuid, ParameterWeightChangeDto> = BTreeMap::new();
    for setting in given {
        let parameter = find_parameter(&setting.parameter, list, heap)?;
        let change = ParameterWeightChangeDto {
            id: parameter.id,
            weight: i64::from(setting.weight),
            version: parameter.version,
        };
        if changes
            .insert(parameter.id, change)
            .is_some_and(|before| before.weight != i64::from(setting.weight))
        {
            return Err(UsageError::ParameterWeightsConflict {
                name: parameter.name.clone(),
            }
            .into());
        }
    }
    Ok(changes.into_values().collect())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn weight(parameter: &str, weight: u16) -> ParameterWeight {
        ParameterWeight {
            parameter: parameter.to_owned(),
            weight,
        }
    }

    #[gtest]
    fn a_parameter_weight_splits_at_the_last_equals_and_trims() {
        expect_that!(parse_parameter_weight("Area=20"), ok(eq(&weight("Area", 20))));
        expect_that!(parse_parameter_weight(" Area = 0 "), ok(eq(&weight("Area", 0))));
        expect_that!(parse_parameter_weight("a=b=5"), ok(eq(&weight("a=b", 5))));
    }

    #[gtest]
    fn a_parameter_weight_needs_a_name_and_a_weight_up_to_100() {
        for raw in [
            "Area", "=5", " =5", "Area=", "Area=x", "Area=101", "Area=-1", "Area=1.5",
        ] {
            expect_that!(
                parse_parameter_weight(raw),
                err(eq(&format!(
                    "--parameter takes NAME=N, such as --parameter Area=20; got `{raw}`"
                )))
            );
        }
        expect_that!(parse_parameter_weight("Area=100"), ok(eq(&weight("Area", 100))));
    }

    #[gtest]
    fn a_flag_weight_is_a_whole_number_from_0_to_100() {
        expect_that!(parse_weight("0"), ok(eq(&0)));
        expect_that!(parse_weight("100"), ok(eq(&100)));
        for raw in ["101", "-1", "", "x", "2.5"] {
            expect_that!(parse_weight(raw), err(anything()));
        }
    }

    fn list() -> googletest::Result<ParameterListDto> {
        let parameter = |id: u128, name: &str, version: u32| {
            serde_json::json!({
                "id": Uuid::from_u128(id), "heap_id": Uuid::new_v4(), "name": name, "weight": 10,
                "options": [], "archived_options": [], "tag_ids": [], "archived_at": null,
                "version": version, "allowed_actions": [],
            })
        };
        Ok(serde_json::from_value(serde_json::json!({
            "heap": { "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "personal", "name": "Personal", "archived": false },
            "parameters": [parameter(1, "Area", 3), parameter(2, "Customer", 1)],
            "archived": [], "allowed_actions": [],
        }))?)
    }

    fn personal() -> HeapHandle {
        HeapHandle {
            team: "ada".to_owned(),
            heap: "personal".to_owned(),
        }
    }

    #[gtest]
    fn the_same_parameter_by_name_or_id_with_one_weight_is_kept_once() -> googletest::Result<()> {
        // Arrange
        let id = Uuid::from_u128(1).to_string();
        let given = [weight("Area", 20), weight("area", 20), weight(&id, 20)];

        // Act
        let changes = resolve_parameter_weights(&given, &list()?, &personal())?;

        // Assert
        verify_that!(changes.len(), eq(1))?;
        verify_that!(changes[0].id, eq(Uuid::from_u128(1)))?;
        verify_that!(changes[0].weight, eq(20))?;
        verify_that!(changes[0].version, eq(3))
    }

    #[gtest]
    fn the_same_parameter_with_two_weights_is_a_usage_error() -> googletest::Result<()> {
        // Arrange
        let given = [weight("Area", 10), weight("area", 20)];

        // Act
        let result = resolve_parameter_weights(&given, &list()?, &personal());

        // Assert
        verify_that!(
            result,
            err(displays_as(eq("--parameter names Area twice with different weights")))
        )
    }

    #[gtest]
    fn an_unknown_parameter_is_a_lookup_error() -> googletest::Result<()> {
        // Arrange
        let given = [weight("Nope", 5)];

        // Act
        let result = resolve_parameter_weights(&given, &list()?, &personal());

        // Assert
        verify_that!(result, err(displays_as(contains_substring("Nope"))))
    }
}
