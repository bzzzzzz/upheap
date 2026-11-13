//! `upheap item list [<text>]`: the heap's ranked list as the web app shows it, its closed items with `--closed`, or
//! every heap's items in one ranked list with `--all-heaps`.

use api_types::common::{ClosedGroupDto, ItemDto, NeedDto, RankedItemDto, SizeDto};
use api_types::items::{
    AcrossHeapsItemDto, AcrossHeapsListDto, AcrossHeapsQuery, ClosedItemsDto, ClosedQuery, RankedListDto, RankedQuery,
};
use api_types::list_view::{BlockedLayoutDto, ListViewDto, RemindersModeDto, SortDto, StateModeDto};
use api_types::parameters::ParameterListDto;
use api_types::tags::TagListDto;
use api_types::views::SavedViewListDto;
use api_types::waiting::ItemHeapDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, UsageError, parse_api_value};
use crate::lookup::{find_view, matches_text, resolve_parameter, resolve_tags};
use crate::output::{Cell, Mode, Output, Tone, shorten};
use crate::{App, CliError};

use super::common::{due_cell, sizes_text, state_text, tags_text};

/// Titles longer than this are cut short in the table, so the columns after them stay on screen.
const TITLE_WIDTH: usize = 60;
const REASON_WIDTH: usize = 40;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only items whose title or description contains this, ignoring case. Not with --json, which prints the
    /// server's list as it came
    #[arg(value_name = "TEXT", conflicts_with = "json")]
    pub text: Option<String>,
    /// Every open item of every heap you can use, in one ranked list, each with its heap. In place of one heap's list
    #[arg(long, conflicts_with_all = ["name", "view", "tag", "closed", "sort", "sort_parameter", "reminders"])]
    pub all_heaps: bool,
    /// Closed items, most recently closed first, in place of the ranked list
    #[arg(long, conflicts_with_all = [
        "view", "tag", "need", "effort", "impact", "sort", "sort_parameter", "open", "in_progress", "blocked",
        "blocked_layout", "reminders",
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
    /// What each group is sorted by: priority, due, need, effort, impact, or parameter with --sort-parameter
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
    /// Whether to list the heap's reminders to come, in a group after the others: hidden or shown. Hidden by
    /// default, but a TEXT search always looks in them
    #[arg(long, value_name = "MODE", value_parser = parse_api_value::<RemindersModeDto>)]
    pub reminders: Option<RemindersModeDto>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

pub async fn run(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if args.all_heaps {
        return list_across_heaps(&args, app, out).await;
    }
    if let (Some(sort), Some(_)) = (args.sort, &args.sort_parameter)
        && sort != SortDto::Parameter
    {
        let sort = serde_json::to_value(sort)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        return Err(UsageError::SortWithSortParameter { sort }.into());
    }
    // Without a view to name the parameter, the flags must.
    if args.sort == Some(SortDto::Parameter) && args.sort_parameter.is_none() && args.view.is_none() {
        return Err(UsageError::SortWithoutParameter.into());
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
    if args.closed {
        return list_closed(&api, args.text.as_deref(), out).await;
    }
    let query = build_query(&args, &api).await?;
    if query.sort == Some(SortDto::Parameter) && query.sort_parameter.is_none() {
        return Err(UsageError::SortWithoutParameter.into());
    }
    let list = api.get::<RankedListDto>(&["items"], &query).await?;
    match out.mode() {
        Mode::Json => out.json(&list.body)?,
        Mode::Quiet => {
            for entry in groups(&list.value).into_iter().flat_map(|(_, entries)| entries) {
                if keeps(&entry.item, args.text.as_deref()) {
                    out.line(&entry.item.key)?;
                }
            }
            for item in list
                .value
                .reminders
                .iter()
                .filter(|item| keeps(item, args.text.as_deref()))
            {
                out.line(&item.key)?;
            }
        }
        Mode::Human => render_ranked(&list.value, &query, args.text.as_deref(), out)?,
    }
    Ok(())
}

/// `--all-heaps`: `GET /me/items` with the view flags given. `--quiet` prints `team/heap/key`, which every item
/// command accepts.
async fn list_across_heaps(args: &ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let query = AcrossHeapsQuery {
        as_of: None,
        open: args.open,
        in_progress: args.in_progress,
        blocked: args.blocked,
        blocked_layout: args.blocked_layout,
        effort: args.effort.clone(),
        impact: args.impact.clone(),
        need: args.need.clone(),
    };
    let list = client
        .get::<AcrossHeapsListDto>(&["me", "items"], &query, &token)
        .await?;
    let text = args.text.as_deref();
    match out.mode() {
        Mode::Json => out.json(&list.body)?,
        Mode::Quiet => {
            for row in across_groups(&list.value).into_iter().flat_map(|(_, rows)| rows) {
                if keeps(&row.entry.item, text) {
                    out.line(&format!("{}/{}", heap_label(&row.heap), row.entry.item.key))?;
                }
            }
        }
        Mode::Human => render_across_heaps(&list.value, &query, text, out)?,
    }
    Ok(())
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
    query.reminders = args.reminders.or(query.reminders);
    // A search finds reminders to come whatever the setting says.
    if args.text.is_some() {
        query.reminders = Some(RemindersModeDto::Shown);
    }
    if let Some(sort) = args.sort {
        query.sort = Some(sort);
        // A view's sort parameter goes with sorting by a parameter, and only with that.
        if sort != SortDto::Parameter {
            query.sort_parameter = None;
        }
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
        reminders: Some(view.reminders),
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

/// The groups of the list across heaps in the order they're shown, with their headings.
fn across_groups(list: &AcrossHeapsListDto) -> [(&'static str, &[AcrossHeapsItemDto]); 3] {
    [
        ("Pinned", &list.pinned),
        ("Ranked", &list.ranked),
        ("Blocked", &list.blocked),
    ]
}

/// `team/heap`, the name every item command takes for a heap.
fn heap_label(heap: &ItemHeapDto) -> String {
    format!("{}/{}", heap.team_slug, heap.slug)
}

fn keeps(item: &ItemDto, text: Option<&str>) -> bool {
    text.is_none_or(|text| matches_text(item, text))
}

/// A row to print: its position in the list shown, the entry, and the heap's `team/heap` when the list spans heaps.
struct ShownRow<'a> {
    position: u32,
    entry: &'a RankedItemDto,
    heap: Option<String>,
}

fn render_ranked(
    list: &RankedListDto,
    query: &RankedQuery,
    text: Option<&str>,
    out: &mut Output,
) -> std::io::Result<()> {
    let shown = groups(list)
        .into_iter()
        .map(|(heading, entries)| {
            let rows = entries.iter().filter(|e| keeps(&e.item, text)).map(|entry| ShownRow {
                position: entry.position,
                entry,
                heap: None,
            });
            (heading, rows.collect::<Vec<_>>())
        })
        .collect();
    let empty = match text {
        Some(text) => format!("No items in {} match \"{text}\".", list.heap.name),
        None => format!("No items in {}.", list.heap.name),
    };
    let view = query.to_view().ok().map(|view| ListViewDto::from(&view));
    let footers: Vec<String> = hidden_footer(list.hidden_count, view).into_iter().collect();
    let reminders: Vec<&ItemDto> = list.reminders.iter().filter(|item| keeps(item, text)).collect();
    render_groups(shown, &reminders, &empty, &footers, out)
}

fn render_across_heaps(
    list: &AcrossHeapsListDto,
    query: &AcrossHeapsQuery,
    text: Option<&str>,
    out: &mut Output,
) -> std::io::Result<()> {
    let mut returned = 0;
    let shown = across_groups(list)
        .into_iter()
        .map(|(heading, rows)| {
            returned += rows.len();
            let rows = rows
                .iter()
                .filter(|row| keeps(&row.entry.item, text))
                .map(|row| ShownRow {
                    position: row.position,
                    entry: &row.entry,
                    heap: Some(heap_label(&row.heap)),
                });
            (heading, rows.collect::<Vec<_>>())
        })
        .collect();
    let empty = match text {
        Some(text) => format!("No items in your heaps match \"{text}\"."),
        None => "No items in your heaps.".to_owned(),
    };
    let view = query.to_view().ok().map(|view| ListViewDto::from(&view));
    let mut footers: Vec<String> = hidden_footer(list.hidden_count, view).into_iter().collect();
    if usize::try_from(list.count).is_ok_and(|count| count > returned) {
        footers.push(format!(
            "Showing the first {returned} of {}; narrow it with --need, --effort or --impact.",
            list.count
        ));
    }
    render_groups(shown, &[], &empty, &footers, out)
}

/// The table of `shown` groups under their headings, then the `reminders` to come under theirs, or `empty` when
/// there is no row at all, then the footers.
fn render_groups(
    shown: Vec<(&'static str, Vec<ShownRow<'_>>)>,
    reminders: &[&ItemDto],
    empty: &str,
    footers: &[String],
    out: &mut Output,
) -> std::io::Result<()> {
    let shown: Vec<_> = shown.into_iter().filter(|(_, rows)| !rows.is_empty()).collect();
    if shown.is_empty() && reminders.is_empty() {
        out.line(empty)?;
    }
    let mut rows = Vec::new();
    for (_, group) in &shown {
        rows.extend(group.iter().map(ranked_row));
    }
    let mut lines = out.render_table(&rows).into_iter();
    // Headings only when there's more than the ranked group to tell apart.
    let headed =
        shown.len() > 1 || !reminders.is_empty() || shown.first().is_some_and(|(heading, _)| *heading != "Ranked");
    for (heading, group) in &shown {
        if headed {
            out.line_toned(heading, Tone::Strong)?;
        }
        for line in lines.by_ref().take(group.len()) {
            out.table_line("", &line)?;
        }
    }
    if !reminders.is_empty() {
        out.line_toned("Reminders to come", Tone::Strong)?;
        let rows: Vec<Vec<Cell>> = reminders.iter().map(|item| reminder_row(item)).collect();
        for line in out.render_table(&rows) {
            out.table_line("", &line)?;
        }
    }
    if !footers.is_empty() {
        out.line("")?;
        for footer in footers {
            out.line_toned(footer, Tone::Dim)?;
        }
    }
    Ok(())
}

/// A reminder to come: key, title, the date it joins the list, tags. It has no position and no reason.
fn reminder_row(item: &ItemDto) -> Vec<Cell> {
    vec![
        Cell::toned(item.key.clone(), Tone::Strong),
        Cell::new(shorten(&item.title, TITLE_WIDTH)),
        Cell::new(item.due.as_ref().map(|due| due.date.clone()).unwrap_or_default()),
        Cell::new(tags_text(item)),
    ]
}

fn ranked_row(row: &ShownRow<'_>) -> Vec<Cell> {
    let entry = row.entry;
    let item = &entry.item;
    let reason = entry
        .contributors
        .first()
        .map(|top| format!("{} ({}%)", top.label, top.share_percent))
        .unwrap_or_default();
    // A blocked item says why, when it has a reason.
    let state = match &item.blocked_reason {
        Some(why) => format!("{}: {}", state_text(item.state), shorten(why, REASON_WIDTH)),
        None => state_text(item.state).to_owned(),
    };
    let mut cells = vec![
        Cell::toned(row.position.to_string(), Tone::Dim),
        Cell::toned(item.key.clone(), Tone::Strong),
    ];
    cells.extend(row.heap.clone().map(Cell::new));
    cells.extend([
        Cell::new(shorten(&item.title, TITLE_WIDTH)),
        Cell::new(state),
        Cell::new(sizes_text(item.effort, item.impact)),
        due_cell(item),
        Cell::new(tags_text(item)),
        Cell::toned(reason, Tone::Dim),
    ]);
    cells
}

/// "3 items hidden by their state (blocked); show them with --blocked shown", when the view hides any.
fn hidden_footer(hidden: u32, view: Option<ListViewDto>) -> Option<String> {
    if hidden == 0 {
        return None;
    }
    let noun = if hidden == 1 { "item" } else { "items" };
    // `view` is the one the server used: the query with the API's defaults for what it left out.
    let Some(view) = view else {
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
            out.line_toned(closed_heading(group), Tone::Strong)?;
        }
        out.table_line("", &line)?;
    }
    if page.next_cursor.is_some() {
        out.line("")?;
        out.line_toned(
            &format!(
                "The {} most recently closed; older ones are in the web app.",
                page.items.len()
            ),
            Tone::Dim,
        )?;
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

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_footer_names_the_hidden_states_and_the_flags_that_show_them() {
        let view = |query: RankedQuery| query.to_view().ok().map(|view| ListViewDto::from(&view));
        let open_hidden = view(RankedQuery {
            open: Some(StateModeDto::Hidden),
            ..RankedQuery::default()
        });

        expect_that!(hidden_footer(0, view(RankedQuery::default())), none());
        expect_that!(
            hidden_footer(1, view(RankedQuery::default())),
            some(eq(
                "1 item hidden by their state (blocked); show them with --blocked shown."
            ))
        );
        expect_that!(
            hidden_footer(3, open_hidden),
            some(eq(
                "3 items hidden by their state (open, blocked); show them with --open shown --blocked shown."
            ))
        );
    }
}
