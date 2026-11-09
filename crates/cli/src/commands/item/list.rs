//! `upheap item list [<text>]`: the heap's ranked list as the web app shows it, or its closed items with `--closed`.

use api_types::common::{ClosedGroupDto, ItemDto, NeedDto, RankedItemDto, SizeDto, StateDto};
use api_types::items::{ClosedItemsDto, ClosedQuery, RankedListDto, RankedQuery};
use api_types::list_view::{BlockedLayoutDto, ListViewDto, SortDto, StateModeDto};
use api_types::me::MeDto;
use api_types::parameters::ParameterListDto;
use api_types::tags::TagListDto;
use api_types::views::SavedViewListDto;
use clap::Args;

use crate::client::{ApiClient, NO_QUERY};
use crate::commands::{HeapArgs, SharedArgs, parse_api_value};
use crate::config::SettingError;
use crate::lookup::{HeapHandle, HeapName, find_view, matches_text, resolve_heap, resolve_parameter, resolve_tags};
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

/// Titles longer than this are cut short in the table, so the columns after them stay on screen.
const TITLE_WIDTH: usize = 60;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only items whose title or description contains this, ignoring case. Not with --json, which prints the
    /// server's list as it came
    #[arg(value_name = "TEXT", conflicts_with = "json")]
    pub text: Option<String>,
    /// Closed items, most recently closed first, in place of the ranked list
    #[arg(long, conflicts_with_all = [
        "view", "tag", "need", "effort", "impact", "sort", "sort_parameter", "open", "in_progress", "blocked",
        "blocked_layout",
    ])]
    pub closed: bool,
    /// A saved view of the heap, by name or ID. Flags given with it change what it shows
    #[arg(long, value_name = "VIEW")]
    pub view: Option<String>,
    /// Keep items with any of these tags, by name. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub tag: Vec<String>,
    /// Keep items with any of these needs: idea, nice_to_have, expected, critical. Repeat or separate with commas
    #[arg(long, value_name = "NEED", value_delimiter = ',', value_parser = parse_api_value::<NeedDto>)]
    pub need: Vec<NeedDto>,
    /// Keep items with any of these efforts: s, m, l, xl. Repeat or separate with commas
    #[arg(long, value_name = "SIZE", value_delimiter = ',', value_parser = parse_api_value::<SizeDto>)]
    pub effort: Vec<SizeDto>,
    /// Keep items with any of these impacts: s, m, l, xl. Repeat or separate with commas
    #[arg(long, value_name = "SIZE", value_delimiter = ',', value_parser = parse_api_value::<SizeDto>)]
    pub impact: Vec<SizeDto>,
    /// What each group is sorted by: priority, due, effort, impact, or parameter with --sort-parameter
    #[arg(long, value_name = "SORT", value_parser = parse_api_value::<SortDto>)]
    pub sort: Option<SortDto>,
    /// The custom parameter to sort by, by name or ID; implies --sort parameter
    #[arg(long, value_name = "PARAMETER")]
    pub sort_parameter: Option<String>,
    /// Where open items go: pinned, shown or hidden. Shown by default
    #[arg(long, value_name = "MODE", value_parser = parse_api_value::<StateModeDto>)]
    pub open: Option<StateModeDto>,
    /// Where in-progress items go: pinned, shown or hidden. Pinned by default
    #[arg(long, value_name = "MODE", value_parser = parse_api_value::<StateModeDto>)]
    pub in_progress: Option<StateModeDto>,
    /// Where blocked items go: pinned, shown or hidden. Hidden by default
    #[arg(long, value_name = "MODE", value_parser = parse_api_value::<StateModeDto>)]
    pub blocked: Option<StateModeDto>,
    /// Whether shown blocked items get their own group or are mixed in: grouped or mixed
    #[arg(long, value_name = "LAYOUT", value_parser = parse_api_value::<BlockedLayoutDto>)]
    pub blocked_layout: Option<BlockedLayoutDto>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

pub async fn run(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let typed = app.heap_name(args.heap.name.as_deref()).ok_or(SettingError::NoHeap)?;
    let name = HeapName::parse(typed)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = match name.to_handle() {
        Some(handle) => handle,
        None => {
            let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
            resolve_heap(name, &me.value)?
        }
    };
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    if args.closed {
        return list_closed(&api, args.text.as_deref(), out).await;
    }
    let query = build_query(&args, &api).await?;
    let list = api.get::<RankedListDto>(&["items"], &query).await?;
    match out.mode() {
        Mode::Json => out.json(&list.body)?,
        Mode::Quiet => {
            for entry in groups(&list.value).into_iter().flat_map(|(_, entries)| entries) {
                if keeps(&entry.item, args.text.as_deref()) {
                    out.line(&entry.item.key)?;
                }
            }
        }
        Mode::Human => render_ranked(&list.value, &query, args.text.as_deref(), out)?,
    }
    Ok(())
}

/// The calls under one heap's path, as one caller.
struct HeapApi<'a> {
    client: &'a ApiClient,
    token: &'a str,
    heap: &'a HeapHandle,
}

impl HeapApi<'_> {
    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        resource: &[&str],
        query: &(impl serde::Serialize + ?Sized),
    ) -> Result<crate::client::Fetched<T>, CliError> {
        let mut path = vec!["teams", self.heap.team.as_str(), "heaps", self.heap.heap.as_str()];
        path.extend_from_slice(resource);
        Ok(self.client.get(&path, query, self.token).await?)
    }
}

/// The list query: the saved view's settings when `--view` names one, with the flags given on top. Tag and
/// parameter names are looked up in the heap.
async fn build_query(args: &ListArgs, api: &HeapApi<'_>) -> Result<RankedQuery, CliError> {
    let mut query = match &args.view {
        Some(name) => {
            let views = api.get::<SavedViewListDto>(&["views"], NO_QUERY).await?;
            to_query(&find_view(name, &views.value, api.heap)?.view)
        }
        None => RankedQuery::default(),
    };
    query.open = args.open.or(query.open);
    query.in_progress = args.in_progress.or(query.in_progress);
    query.blocked = args.blocked.or(query.blocked);
    query.blocked_layout = args.blocked_layout.or(query.blocked_layout);
    if let Some(sort) = args.sort {
        query.sort = Some(sort);
        // A view's sort parameter goes with the view's sort.
        query.sort_parameter = None;
    }
    if let Some(name) = &args.sort_parameter {
        let parameters = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
        query.sort = Some(SortDto::Parameter);
        query.sort_parameter = Some(resolve_parameter(name, &parameters.value, api.heap)?);
    }
    if !args.tag.is_empty() {
        let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
        query.tag = resolve_tags(&args.tag, &tags.value, api.heap)?;
    }
    if !args.need.is_empty() {
        query.need.clone_from(&args.need);
    }
    if !args.effort.is_empty() {
        query.effort.clone_from(&args.effort);
    }
    if !args.impact.is_empty() {
        query.impact.clone_from(&args.impact);
    }
    Ok(query)
}

/// A saved view as a list query: the same settings under the same names.
fn to_query(view: &ListViewDto) -> RankedQuery {
    RankedQuery {
        as_of: None,
        open: Some(view.open),
        in_progress: Some(view.in_progress),
        blocked: Some(view.blocked),
        blocked_layout: Some(view.blocked_layout),
        sort: Some(view.sort),
        sort_parameter: view.sort_parameter,
        tag: view.tag.clone(),
        effort: view.effort.clone(),
        impact: view.impact.clone(),
        need: view.need.clone(),
        param: view.param.clone(),
    }
}

/// The list's groups in the order they're shown, with their headings.
fn groups(list: &RankedListDto) -> [(&'static str, &[RankedItemDto]); 3] {
    [
        ("Pinned", &list.pinned),
        ("Ranked", &list.ranked),
        ("Blocked", &list.blocked),
    ]
}

fn keeps(item: &ItemDto, text: Option<&str>) -> bool {
    text.is_none_or(|text| matches_text(item, text))
}

fn render_ranked(
    list: &RankedListDto,
    query: &RankedQuery,
    text: Option<&str>,
    out: &mut Output,
) -> std::io::Result<()> {
    let shown: Vec<(&str, Vec<&RankedItemDto>)> = groups(list)
        .into_iter()
        .map(|(heading, entries)| {
            (
                heading,
                entries.iter().filter(|e| keeps(&e.item, text)).collect::<Vec<_>>(),
            )
        })
        .filter(|(_, entries)| !entries.is_empty())
        .collect();
    if shown.is_empty() {
        out.line(&match text {
            Some(text) => format!("No items in {} match \"{text}\".", list.heap.name),
            None => format!("No items in {}.", list.heap.name),
        })?;
    }
    let mut rows = Vec::new();
    for (_, entries) in &shown {
        rows.extend(entries.iter().map(|entry| ranked_row(entry)));
    }
    let mut lines = out.render_table(&rows).into_iter();
    // Headings only when there's more than the ranked group to tell apart.
    let headed = shown.len() > 1 || shown.first().is_some_and(|(heading, _)| *heading != "Ranked");
    for (heading, entries) in &shown {
        if headed {
            out.line(&out.paint(heading, Tone::Strong))?;
        }
        for line in lines.by_ref().take(entries.len()) {
            out.line(&line)?;
        }
    }
    if let Some(footer) = hidden_footer(list.hidden_count, query) {
        out.line("")?;
        out.line(&out.paint(&footer, Tone::Dim))?;
    }
    Ok(())
}

fn ranked_row(entry: &RankedItemDto) -> Vec<Cell> {
    let item = &entry.item;
    let reason = entry
        .contributors
        .first()
        .map(|top| format!("{} ({}%)", top.label, top.share_percent))
        .unwrap_or_default();
    vec![
        Cell::toned(entry.position.to_string(), Tone::Dim),
        Cell::toned(item.key.clone(), Tone::Strong),
        Cell::new(shorten(&item.title, TITLE_WIDTH)),
        Cell::new(state_text(item.state)),
        Cell::new(sizes_text(item.effort, item.impact)),
        due_cell(item),
        Cell::new(tags_text(item)),
        Cell::toned(reason, Tone::Dim),
    ]
}

/// "3 items hidden by their state (blocked); show them with --blocked shown", when the view hides any.
fn hidden_footer(hidden: u32, query: &RankedQuery) -> Option<String> {
    if hidden == 0 {
        return None;
    }
    let noun = if hidden == 1 { "item" } else { "items" };
    // The view the server used: the query with the API's defaults for what it left out.
    let Ok(view) = query.to_view().map(|view| ListViewDto::from(&view)) else {
        return Some(format!("{hidden} {noun} hidden by their state."));
    };
    let hidden_states: Vec<(&str, &str)> = [
        (view.open, "open", "--open"),
        (view.in_progress, "in progress", "--in-progress"),
        (view.blocked, "blocked", "--blocked"),
    ]
    .into_iter()
    .filter(|(mode, _, _)| *mode == StateModeDto::Hidden)
    .map(|(_, state, flag)| (state, flag))
    .collect();
    let states: Vec<&str> = hidden_states.iter().map(|(state, _)| *state).collect();
    let flags: Vec<String> = hidden_states.iter().map(|(_, flag)| format!("{flag} shown")).collect();
    Some(format!(
        "{hidden} {noun} hidden by their state ({}); show them with {}.",
        states.join(", "),
        flags.join(" ")
    ))
}

async fn list_closed(api: &HeapApi<'_>, text: Option<&str>, out: &mut Output) -> Result<(), CliError> {
    let page = api
        .get::<ClosedItemsDto>(&["items", "closed"], &ClosedQuery::default())
        .await?;
    let items: Vec<&ItemDto> = page.value.items.iter().filter(|item| keeps(item, text)).collect();
    match out.mode() {
        Mode::Json => out.json(&page.body)?,
        Mode::Quiet => {
            for item in items {
                out.line(&item.key)?;
            }
        }
        Mode::Human => render_closed(&page.value, &items, text, out)?,
    }
    Ok(())
}

fn render_closed(
    page: &ClosedItemsDto,
    items: &[&ItemDto],
    text: Option<&str>,
    out: &mut Output,
) -> std::io::Result<()> {
    if items.is_empty() {
        out.line(&match text {
            Some(text) => format!("No closed items in {} match \"{text}\".", page.heap.name),
            None => format!("No closed items in {}.", page.heap.name),
        })?;
        return Ok(());
    }
    let rows: Vec<Vec<Cell>> = items
        .iter()
        .map(|item| {
            vec![
                Cell::toned(item.key.clone(), Tone::Strong),
                Cell::new(shorten(&item.title, TITLE_WIDTH)),
                Cell::new(state_text(item.state)),
                Cell::new(item.closed_label.clone().unwrap_or_default()),
                Cell::new(tags_text(item)),
            ]
        })
        .collect();
    let lines = out.render_table(&rows);
    let mut group = None;
    for (item, line) in items.iter().zip(lines) {
        // Consecutive items of one group go under its heading, as the API asks.
        if item.closed_group != group {
            group = item.closed_group;
            out.line(&out.paint(closed_heading(group), Tone::Strong))?;
        }
        out.line(&line)?;
    }
    if page.next_cursor.is_some() {
        out.line("")?;
        out.line(&out.paint(
            &format!(
                "The {} most recently closed; older ones are in the web app.",
                page.items.len()
            ),
            Tone::Dim,
        ))?;
    }
    Ok(())
}

fn closed_heading(group: Option<ClosedGroupDto>) -> &'static str {
    match group {
        Some(ClosedGroupDto::Today) => "Today",
        Some(ClosedGroupDto::ThisWeek) => "This week",
        Some(ClosedGroupDto::Earlier) | None => "Earlier",
        Some(ClosedGroupDto::Unknown) => "Closed",
    }
}

fn state_text(state: StateDto) -> &'static str {
    match state {
        StateDto::Open => "open",
        StateDto::InProgress => "in progress",
        StateDto::Blocked => "blocked",
        StateDto::Done => "done",
        StateDto::WontDo => "won't do",
    }
}

/// Effort and impact as `s/m`, with `-` for one that's unset.
fn sizes_text(effort: Option<SizeDto>, impact: Option<SizeDto>) -> String {
    let size = |size: Option<SizeDto>| match size {
        Some(SizeDto::S) => "s",
        Some(SizeDto::M) => "m",
        Some(SizeDto::L) => "l",
        Some(SizeDto::Xl) => "xl",
        None => "-",
    };
    format!("{}/{}", size(effort), size(impact))
}

fn due_cell(item: &ItemDto) -> Cell {
    match &item.due {
        Some(due) if due.overdue => Cell::toned(due.label.clone(), Tone::Alert),
        Some(due) => Cell::new(due.label.clone()),
        None => Cell::default(),
    }
}

fn tags_text(item: &ItemDto) -> String {
    item.tags
        .iter()
        .map(|tag| tag.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `text` cut to `width` characters, the last one an ellipsis, when it's longer.
fn shorten(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_footer_names_the_hidden_states_and_the_flags_that_show_them() {
        let default = RankedQuery::default();
        let open_hidden = RankedQuery {
            open: Some(StateModeDto::Hidden),
            ..RankedQuery::default()
        };

        expect_that!(hidden_footer(0, &default), none());
        expect_that!(
            hidden_footer(1, &default),
            some(eq(
                "1 item hidden by their state (blocked); show them with --blocked shown."
            ))
        );
        expect_that!(
            hidden_footer(3, &open_hidden),
            some(eq(
                "3 items hidden by their state (open, blocked); show them with --open shown --blocked shown."
            ))
        );
    }

    #[gtest]
    fn long_titles_are_cut_with_an_ellipsis() {
        expect_that!(shorten("short", 10), eq("short"));
        expect_that!(shorten("exactly10!", 10), eq("exactly10!"));
        expect_that!(shorten("a bit too long", 10), eq("a bit too…"));
    }
}
