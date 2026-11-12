//! `upheap parameter edit <parameter>`: change a parameter's name, weight, tags or option order.
//!
//! The API has one endpoint for each kind of change, so this makes one write per kind, in a fixed order, each at the
//! version the last one returned. A failure after the first write leaves the earlier ones saved, and says which.

use api_types::parameters::{
    ArrangeOptionsDto, LinkTagsDto, OptionDto, ParameterDto, ParameterListDto, ParameterPatchDto,
};
use api_types::tags::TagListDto;
use clap::Args;

use super::{arrangement, describe_weight_flag, weight_parser};
use crate::client::{Fetched, NO_QUERY};
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::item::common::merge_tag_ids;
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::lookup::{HeapHandle, LookupError, find_option, find_parameter, resolve_tags};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    /// A new name, 1 to 50 characters, not used by another parameter in use ignoring case
    #[arg(long, id = "new_name", value_name = "NAME")]
    pub name: Option<String>,
    #[arg(long, value_name = "N", value_parser = weight_parser(), help = describe_weight_flag())]
    pub weight: Option<u8>,
    /// Suggest the parameter for a tag of the heap, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub tag: Vec<String>,
    /// Stop suggesting it for a tag, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub untag: Vec<String>,
    /// Every option of the parameter, highest priority first, by label or ID, separated by commas
    #[arg(long, value_name = "OPTIONS", value_delimiter = ',')]
    pub order: Vec<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

impl EditArgs {
    /// Whether any change flag is given.
    fn has_changes(&self) -> bool {
        self.name.is_some()
            || self.weight.is_some()
            || !self.tag.is_empty()
            || !self.untag.is_empty()
            || !self.order.is_empty()
    }
}

/// The kinds of change a parameter takes, each its own write, in the order the edit makes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// The name and the weight.
    Details,
    Tags,
    Order,
}

impl Part {
    /// What the part changes, as it reads after "its new": "name and weight".
    fn saved(self) -> &'static str {
        match self {
            Self::Details => "name and weight",
            Self::Tags => "tags",
            Self::Order => "option order",
        }
    }

    /// What the part changes, as it reads after "but not its": "tags".
    fn unsaved(self) -> &'static str {
        match self {
            Self::Details => "name or weight",
            Self::Tags => "tags",
            Self::Order => "order",
        }
    }

    /// The flags that ask for the part.
    fn flags(self) -> &'static str {
        match self {
            Self::Details => "--name/--weight",
            Self::Tags => "--tag/--untag",
            Self::Order => "--order",
        }
    }
}

/// One write of the edit, with the body it sends.
enum Write {
    Details(ParameterPatchDto),
    Tags(LinkTagsDto),
    Order(ArrangeOptionsDto),
}

impl Write {
    fn part(&self) -> Part {
        match self {
            Self::Details(_) => Part::Details,
            Self::Tags(_) => Part::Tags,
            Self::Order(_) => Part::Order,
        }
    }

    /// Sends the write for the parameter `id`, which is at `version`.
    async fn send(&self, api: &HeapApi<'_>, id: &str, version: u32) -> Result<Fetched<ParameterDto>, CliError> {
        match self {
            Self::Details(body) => api.patch(&["parameters", id], version, body).await,
            Self::Tags(body) => api.put(&["parameters", id, "tags"], version, body).await,
            Self::Order(body) => api.patch(&["parameters", id, "options"], version, body).await,
        }
    }
}

/// Changes what the flags name and leaves the rest. Everything that can be checked is checked before the first
/// write. Human output says whether the parameter was renamed, updated or left as it was; `--quiet` prints nothing
/// and `--json` the parameter as the last write returned it.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if !args.has_changes() {
        if out.mode() == Mode::Human {
            out.line("Nothing to change: give --name, --weight, --tag, --untag or --order.")?;
        }
        return Ok(());
    }
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
    let tag_ids = if args.tag.is_empty() && args.untag.is_empty() {
        None
    } else {
        let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
        let added = resolve_tags(&args.tag, &tags.value, &heap).map_err(LookupError::offering_add)?;
        let removed = resolve_tags(&args.untag, &tags.value, &heap)?;
        if let Some(position) = added.iter().position(|id| removed.contains(id)) {
            return Err(UsageError::TagAndUntag {
                name: args.tag.get(position).cloned().unwrap_or_default(),
            }
            .into());
        }
        Some(merge_tag_ids(parameter.tag_ids.iter().copied(), added, &removed))
    };
    let order = if args.order.is_empty() {
        None
    } else {
        Some(arrangement(&check_order(&args.order, parameter, &heap)?, None))
    };
    let mut writes = Vec::new();
    if args.name.is_some() || args.weight.is_some() {
        writes.push(Write::Details(ParameterPatchDto {
            name: args.name,
            weight: args.weight.map(i64::from),
        }));
    }
    writes.extend(tag_ids.map(|tag_ids| Write::Tags(LinkTagsDto { tag_ids })));
    writes.extend(order.map(Write::Order));

    let id = parameter.id.to_string();
    let mut latest: Option<Fetched<ParameterDto>> = None;
    let mut saved = Vec::new();
    for (index, write) in writes.iter().enumerate() {
        let version = latest.as_ref().map_or(parameter.version, |last| last.value.version);
        match write.send(&api, &id, version).await {
            Ok(done) => latest = Some(done),
            Err(err) => {
                if out.mode() != Mode::Json {
                    let current = latest.as_ref().map_or(&parameter.name, |last| &last.value.name);
                    let left: Vec<Part> = writes[index..].iter().map(Write::part).collect();
                    if let Some(note) = partial_note(current, &saved, &left) {
                        out.note(&note)?;
                    }
                }
                return Err(err);
            }
        }
        saved.push(write.part());
    }
    let Some(edited) = latest else {
        // Every flag given makes a write, so a command that got this far has one.
        return Ok(());
    };
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&edit_line(parameter, &edited.value))?,
    }
    Ok(())
}

/// The options of `parameter` in the order `typed` gives, each named by label or ID. Every option in use must be
/// named once.
fn check_order<'p>(
    typed: &[String],
    parameter: &'p ParameterDto,
    heap: &HeapHandle,
) -> Result<Vec<&'p OptionDto>, CliError> {
    let mut ordered: Vec<&OptionDto> = Vec::with_capacity(typed.len());
    for name in typed {
        let option = find_option(name.trim(), parameter, heap)?;
        if ordered.iter().any(|seen| seen.id == option.id) {
            return Err(UsageError::OrderRepeatsOption {
                option: option.label.clone(),
            }
            .into());
        }
        ordered.push(option);
    }
    let missing: Vec<String> = parameter
        .options
        .iter()
        .filter(|option| !ordered.iter().any(|named| named.id == option.id))
        .map(|option| option.label.clone())
        .collect();
    if !missing.is_empty() {
        return Err(UsageError::OrderMissesOptions {
            parameter: parameter.name.clone(),
            missing,
        }
        .into());
    }
    Ok(ordered)
}

/// What to tell on stderr when a write failed after others were saved: what was saved, what wasn't, and the flags
/// to run the edit again with. `None` when nothing was saved, since then the parameter is as it was.
fn partial_note(name: &str, saved: &[Part], left: &[Part]) -> Option<String> {
    if saved.is_empty() || left.is_empty() {
        return None;
    }
    let did = saved.iter().map(|part| part.saved()).collect::<Vec<_>>().join(" and ");
    let not = left.iter().map(|part| part.unsaved()).collect::<Vec<_>>().join(" or ");
    let flags = left.iter().map(|part| part.flags()).collect::<Vec<_>>().join(" and ");
    Some(format!(
        "Parameter \"{name}\" got its new {did}, but not its {not}; run the edit again with {flags}."
    ))
}

/// What the edit did, from the parameter as read and as the last write returned it.
fn edit_line(before: &ParameterDto, after: &ParameterDto) -> String {
    if after.version == before.version {
        format!("No change to parameter \"{}\".", after.name)
    } else if after.name != before.name {
        format!("Renamed parameter \"{}\" to \"{}\".", before.name, after.name)
    } else {
        format!("Updated parameter \"{}\".", after.name)
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn handle() -> HeapHandle {
        HeapHandle {
            team: "ada".to_owned(),
            heap: "personal".to_owned(),
        }
    }

    fn area(name: &str, version: u32) -> googletest::Result<ParameterDto> {
        let option = |id: u128, label: &str| json!({ "id": Uuid::from_u128(id), "label": label });
        Ok(serde_json::from_value(json!({
            "id": Uuid::from_u128(1), "heap_id": Uuid::new_v4(), "name": name, "weight": 10,
            "options": [option(11, "Backend"), option(12, "Frontend"), option(13, "Server")],
            "archived_options": [], "tag_ids": [], "archived_at": null, "version": version, "allowed_actions": [],
        }))?)
    }

    fn typed(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[gtest]
    fn an_order_naming_every_option_once_is_the_options_in_that_order() -> googletest::Result<()> {
        let parameter = area("Area", 1)?;

        let ordered = check_order(
            &typed(&["server", " Backend", &Uuid::from_u128(12).to_string()]),
            &parameter,
            &handle(),
        );

        let labels: Vec<&str> = ordered
            .as_ref()
            .map_or(Vec::new(), |o| o.iter().map(|o| o.label.as_str()).collect());
        verify_that!(labels, elements_are![eq(&"Server"), eq(&"Backend"), eq(&"Frontend")])
    }

    #[gtest]
    fn an_order_that_leaves_options_out_names_them() -> googletest::Result<()> {
        let parameter = area("Area", 1)?;

        let ordered = check_order(&typed(&["Server"]), &parameter, &handle());

        verify_that!(
            ordered,
            err(displays_as(eq(
                "--order must name every option of Area once: missing Backend, Frontend"
            )))
        )
    }

    #[gtest]
    fn an_order_that_names_an_option_twice_says_which() -> googletest::Result<()> {
        let parameter = area("Area", 1)?;

        let ordered = check_order(
            &typed(&["Backend", "BACKEND", "Frontend", "Server"]),
            &parameter,
            &handle(),
        );

        verify_that!(ordered, err(displays_as(eq("--order names Backend twice"))))
    }

    #[gtest]
    fn an_order_that_names_an_unknown_option_is_a_lookup_error() -> googletest::Result<()> {
        let parameter = area("Area", 1)?;

        let ordered = check_order(&typed(&["Backend", "Nope"]), &parameter, &handle());

        verify_that!(
            ordered,
            err(matches_pattern!(CliError::Lookup(matches_pattern!(
                LookupError::UnknownOption { .. }
            ))))
        )
    }

    #[gtest]
    fn a_failure_after_a_write_says_what_was_saved_and_what_was_not() {
        use Part::{Details, Order, Tags};

        expect_that!(
            partial_note("Area", &[Details], &[Tags, Order]),
            some(eq(
                "Parameter \"Area\" got its new name and weight, but not its tags or order; run the edit again with --tag/--untag and --order."
            ))
        );
        expect_that!(
            partial_note("Area", &[Details], &[Tags]),
            some(eq(
                "Parameter \"Area\" got its new name and weight, but not its tags; run the edit again with --tag/--untag."
            ))
        );
        expect_that!(
            partial_note("Area", &[Details], &[Order]),
            some(eq(
                "Parameter \"Area\" got its new name and weight, but not its order; run the edit again with --order."
            ))
        );
        expect_that!(
            partial_note("Area", &[Details, Tags], &[Order]),
            some(eq(
                "Parameter \"Area\" got its new name and weight and tags, but not its order; run the edit again with --order."
            ))
        );
        expect_that!(
            partial_note("Area", &[Tags], &[Order]),
            some(eq(
                "Parameter \"Area\" got its new tags, but not its order; run the edit again with --order."
            ))
        );
    }

    #[gtest]
    fn a_failure_of_the_first_write_has_nothing_to_say() {
        expect_that!(partial_note("Area", &[], &[Part::Details, Part::Tags]), none());
    }

    #[gtest]
    fn the_edit_line_says_renamed_updated_or_no_change() -> googletest::Result<()> {
        let before = area("Area", 1)?;

        verify_that!(
            edit_line(&before, &area("Area", 1)?),
            eq("No change to parameter \"Area\".")
        )?;
        verify_that!(
            edit_line(&before, &area("Team", 2)?),
            eq("Renamed parameter \"Area\" to \"Team\".")
        )?;
        verify_that!(edit_line(&before, &area("Area", 2)?), eq("Updated parameter \"Area\"."))
    }
}
