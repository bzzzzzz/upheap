//! `upheap item list [<text>]`: the heap's ranked list as the web app shows it, its closed items with `--closed`, or
//! every heap's items in one ranked list with `--all-heaps`. With `--blocker gone` either one lists only the blocked
//! items whose blocker was closed or moved away, filtered here on what the list carries.

use api_types::common::{BlockerGoneHowDto, ClosedGroupDto, ItemDto, NeedDto, RankedItemDto, SizeDto};
use api_types::items::{
    AcrossHeapsItemDto, AcrossHeapsListDto, AcrossHeapsQuery, ClosedItemsDto, ClosedQuery, RankedListDto, RankedQuery,
};
use api_types::list_view::{BlockedLayoutDto, ListViewDto, RemindersModeDto, SortDto, StateModeDto};
use api_types::parameters::ParameterListDto;
use api_types::tags::{TagListDto, TeamTagListDto};
use api_types::turns::{AgentStateDto, GoalDto};
use api_types::views::SavedViewListDto;
use api_types::waiting::ItemHeapDto;
use clap::{Args, ValueEnum};

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, UsageError, parse_api_value};
use crate::lookup::{find_team_tag, find_view, matches_text, resolve_parameter, resolve_tags};
use crate::output::{Cell, Mode, Output, Tone, shorten};
use crate::{App, CliError};

use super::common::{due_cell, kind_text, sizes_text, state_text, tags_text, with_agent_text};

/// Titles longer than this are cut short in the table, so the columns after them stay on screen.
const TITLE_WIDTH: usize = 60;
const REASON_WIDTH: usize = 40;

/// What `--blocker` keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BlockerFilter {
    /// Blocked items whose blocker was closed or moved away.
    Gone,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only items whose title or description contains this, ignoring case. Not with --json, which prints the
    /// server's list as it came
    #[arg(value_name = "TEXT", conflicts_with = "json")]
    pub text: Option<String>,
    /// Every open item of every heap you can use, in one ranked list, each with its heap. In place of one heap's list
    #[arg(long, conflicts_with_all = ["name", "view", "closed", "sort", "sort_parameter", "reminders"])]
    pub all_heaps: bool,
    /// With --all-heaps, only this team's heaps, by its slug
    #[arg(long, value_name = "TEAM", requires = "all_heaps")]
    pub team: Option<String>,
    /// Closed items, most recently closed first, in place of the ranked list
    #[arg(long, conflicts_with_all = [
        "view", "tag", "need", "effort", "impact", "agent", "going_to", "sort", "sort_parameter", "open", "in_progress", "blocked",
        "blocked_layout", "reminders", "blocker",
    ])]
    pub closed: bool,
    /// A saved view of the heap, by name or ID. Flags given with it change what it shows
    #[arg(long, value_name = "VIEW")]
    pub view: Option<String>,
    /// Keep items with any of these tags, by name. Repeat for more. With --all-heaps, needs --team and takes that
    /// team's tags
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
    /// Keep items in any of these agent states: running, queued, waits, watching, stuck, none. Repeat or separate with commas
    #[arg(long, value_name = "STATE", value_delimiter = ',', value_parser = parse_api_value::<AgentStateDto>)]
    pub agent: Vec<AgentStateDto>,
    /// Keep items whose agent's turn goes to any of these: with_context, defined, done. Repeat or separate with commas
    #[arg(long, value_name = "GOAL", value_delimiter = ',', value_parser = parse_api_value::<GoalDto>)]
    pub going_to: Vec<GoalDto>,
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
    /// Only blocked items whose blocker was closed or moved away, which wait for you: gone. Unblock one with
    /// `upheap item reopen`
    #[arg(long, value_name = "WHICH", value_enum)]
    pub blocker: Option<BlockerFilter>,
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
    let mut query = build_query(&args, &api).await?;
    if args.blocker.is_some() {
        query.open = Some(StateModeDto::Hidden);
        query.in_progress = Some(StateModeDto::Hidden);
        query.blocked = Some(StateModeDto::Shown);
        // A blocked reminder that hasn't arrived yet comes in the reminders, not with the blocked items.
        query.reminders = Some(RemindersModeDto::Shown);
    }
    if query.sort == Some(SortDto::Parameter) && query.sort_parameter.is_none() {
        return Err(UsageError::SortWithoutParameter.into());
    }
    let list = api.get::<RankedListDto>(&["items"], &query).await?;
    if args.blocker.is_some() {
        let rows = groups(&list.value)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .map(|entry| &entry.item)
            .chain(&list.value.reminders)
            .filter(|item| item.blocker_gone.is_some() && keeps(item, args.text.as_deref()))
            .map(|item| (item.key.clone(), item))
            .collect();
        return report_gone(rows, None, &list.body, out);
    }
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
    if !args.tag.is_empty() && args.team.is_none() {
        return Err(UsageError::AllHeapsTagWithoutTeam.into());
    }
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let gone = args.blocker.is_some();
    let tag = match &args.team {
        Some(team) if !args.tag.is_empty() => {
            let api = TeamApi {
                client: &client,
                token: &token,
                team,
            };
            let tags = api.get::<TeamTagListDto>(&["tags"]).await?;
            args.tag
                .iter()
                .map(|name| find_team_tag(name, &tags.value, team).map(|tag| tag.id))
                .collect::<Result<_, _>>()?
        }
        _ => Vec::new(),
    };
    let query = AcrossHeapsQuery {
        team: args.team.clone(),
        as_of: None,
        open: if gone { Some(StateModeDto::Hidden) } else { args.open },
        in_progress: if gone {
            Some(StateModeDto::Hidden)
        } else {
            args.in_progress
        },
        blocked: if gone { Some(StateModeDto::Shown) } else { args.blocked },
        blocked_layout: args.blocked_layout,
        tag,
        effort: args.effort.clone(),
        impact: args.impact.clone(),
        need: args.need.clone(),
        param: Vec::new(),
        agent: args.agent.clone(),
        going_to: args.going_to.clone(),
    };
    let list = client
        .get::<AcrossHeapsListDto>(&["me", "items"], &query, &token)
        .await?;
    let text = args.text.as_deref();
    if gone {
        let all: Vec<&AcrossHeapsItemDto> = across_groups(&list.value)
            .into_iter()
            .flat_map(|(_, rows)| rows)
            .collect();
        let returned = all.len();
        let rows = all
            .into_iter()
            .filter(|row| row.entry.item.blocker_gone.is_some() && keeps(&row.entry.item, text))
            .map(|row| {
                (
                    format!("{}/{}", heap_label(&row.heap), row.entry.item.key),
                    &row.entry.item,
                )
            })
            .collect();
        return report_gone(rows, showing_footer(returned, list.value.count), &list.body, out);
    }
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
    if !args.agent.is_empty() {
        query.agent.clone_from(&args.agent);
    }
    if !args.going_to.is_empty() {
        query.going_to.clone_from(&args.going_to);
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
        // Saved views don't carry the agent filters.
        agent: Vec::new(),
        going_to: Vec::new(),
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
    footers.extend(showing_footer(returned, list.count));
    render_groups(shown, &[], &empty, &footers, out)
}

/// "Showing the first 50 of 80", when the server sent fewer rows than the list has.
fn showing_footer(returned: usize, count: u32) -> Option<String> {
    usize::try_from(count)
        .is_ok_and(|count| count > returned)
        .then(|| format!("Showing the first {returned} of {count}; narrow it with --need, --effort or --impact."))
}

/// `--blocker gone`: the kept `rows`, each a name and its item, as one table with the unblock footer, `--quiet`
/// as the names, and `--json` as the server's `body`.
fn report_gone(
    rows: Vec<(String, &ItemDto)>,
    truncated: Option<String>,
    body: &str,
    out: &mut Output,
) -> Result<(), CliError> {
    match out.mode() {
        Mode::Json => out.json(body)?,
        Mode::Quiet => {
            for (name, _) in &rows {
                out.line(name)?;
            }
        }
        Mode::Human => {
            if rows.is_empty() {
                out.line("No items wait on a closed blocker.")?;
                if let Some(footer) = truncated {
                    out.line_toned(&footer, Tone::Dim)?;
                }
                return Ok(());
            }
            let cells: Vec<Vec<Cell>> = rows
                .iter()
                .map(|(name, item)| {
                    vec![
                        Cell::toned(name.clone(), Tone::Strong),
                        Cell::new(shorten(&item.title, TITLE_WIDTH)),
                        Cell::new(gone_text(item)),
                    ]
                })
                .collect();
            for line in out.render_table(&cells) {
                out.table_line("", &line)?;
            }
            out.line("")?;
            out.line_toned("Unblock one with upheap item reopen <key>.", Tone::Dim)?;
            if let Some(footer) = truncated {
                out.line_toned(&footer, Tone::Dim)?;
            }
        }
    }
    Ok(())
}

/// What the item waits for and what happened to it, in the web app's words.
fn gone_text(item: &ItemDto) -> String {
    let moved_to = item
        .blocker_gone
        .as_ref()
        .and_then(|gone| gone.moved_to.as_ref())
        .map(|heap| heap.name.as_str());
    let how = item
        .blocker_gone
        .as_ref()
        .map_or(BlockerGoneHowDto::Unknown, |gone| gone.how);
    let words = match (how, moved_to) {
        (BlockerGoneHowDto::Done, Some(heap)) => format!("closed as done in {heap}"),
        (BlockerGoneHowDto::Done, None) => "closed as done".to_owned(),
        (BlockerGoneHowDto::WontDo, Some(heap)) => format!("closed as won't do in {heap}"),
        (BlockerGoneHowDto::WontDo, None) => "closed as won't do".to_owned(),
        (BlockerGoneHowDto::Became, Some(heap)) => format!("became other items in {heap}"),
        (BlockerGoneHowDto::Became, None) => "which became other items".to_owned(),
        (BlockerGoneHowDto::Moved, Some(heap)) => format!("moved to {heap}"),
        (BlockerGoneHowDto::Moved, None) => "moved to another team".to_owned(),
        (BlockerGoneHowDto::Unknown, _) => "gone".to_owned(),
    };
    match &item.blocked_by {
        Some(blocker) => format!("Waits for \"{}\", {words}", shorten(&blocker.title, TITLE_WIDTH)),
        None => format!("Waits for an item that {words}"),
    }
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
    // A blocked item says why, when it has a reason, before saying an agent has it.
    let blocked_why = item
        .blocked_reason
        .as_ref()
        .map(|why| format!(": {}", shorten(why, REASON_WIDTH)))
        .unwrap_or_default();
    let state = format!("{}{blocked_why}{}", state_text(item.state), with_agent_text(item));
    // Ticked of open and ticked steps, as the web row shows them; empty for a note and a task with none.
    let steps = item
        .step_progress
        .as_ref()
        .map(|progress| format!("{}/{}", progress.ticked, progress.total))
        .unwrap_or_default();
    let waits = if item.waits_for_you { "waits for you" } else { "" };
    let mut cells = vec![
        Cell::toned(row.position.to_string(), Tone::Dim),
        Cell::toned(item.key.clone(), Tone::Strong),
    ];
    cells.extend(row.heap.clone().map(Cell::new));
    cells.extend([
        Cell::new(kind_text(item.kind)),
        Cell::new(shorten(&item.title, TITLE_WIDTH)),
        Cell::toned(steps, Tone::Dim),
        Cell::new(state),
        Cell::toned(waits, Tone::Strong),
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

    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;

    /// An item blocked on `blocker` (`null` for none), whose blocker is gone as `gone` says.
    fn blocked_item(blocker: Value, gone: Value) -> ItemDto {
        serde_json::from_value(json!({
            "id": Uuid::new_v4(), "key": "k3x9a", "heap_id": Uuid::new_v4(), "title": "Ship it",
            "description": null, "kind": "note", "stage": "as_written", "stage_choices": [], "state": "blocked",
            "blocked_by": blocker, "blocker_gone": gone, "need": "expected", "effort": null, "impact": null,
            "due": null, "tags": [], "parameters": [], "created_at": 0, "created_label": "today",
            "created_by": null, "closed_at": null, "closed_label": null, "closed_by": null, "closed_group": null,
            "steps": [], "step_progress": null, "version": 1, "allowed_actions": [],
        }))
        .expect("an item")
    }

    fn blocker() -> Value {
        json!({ "id": Uuid::new_v4(), "key": "p4q7r", "title": "Fix the build", "kind": "note", "state": "wont_do" })
    }

    fn heap(name: &str) -> Value {
        json!({ "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "other", "name": name, "archived": false })
    }

    #[gtest]
    fn the_blocker_gone_text_says_what_happened_to_the_blocker() {
        let text = |how: &str, moved_to: Value, blocker: Value| {
            gone_text(&blocked_item(blocker, json!({ "how": how, "moved_to": moved_to })))
        };
        let other = || heap("Other");

        expect_that!(
            text("done", Value::Null, blocker()),
            eq("Waits for \"Fix the build\", closed as done")
        );
        expect_that!(
            text("done", other(), blocker()),
            eq("Waits for \"Fix the build\", closed as done in Other")
        );
        expect_that!(
            text("wont_do", Value::Null, blocker()),
            eq("Waits for \"Fix the build\", closed as won't do")
        );
        expect_that!(
            text("wont_do", other(), blocker()),
            eq("Waits for \"Fix the build\", closed as won't do in Other")
        );
        expect_that!(
            text("became", Value::Null, blocker()),
            eq("Waits for \"Fix the build\", which became other items")
        );
        expect_that!(
            text("became", other(), blocker()),
            eq("Waits for \"Fix the build\", became other items in Other")
        );
        expect_that!(
            text("moved", other(), blocker()),
            eq("Waits for \"Fix the build\", moved to Other")
        );
        expect_that!(
            text("moved", Value::Null, blocker()),
            eq("Waits for \"Fix the build\", moved to another team")
        );
        expect_that!(
            text("moved", Value::Null, Value::Null),
            eq("Waits for an item that moved to another team")
        );
        expect_that!(
            text("unknown", Value::Null, blocker()),
            eq("Waits for \"Fix the build\", gone")
        );
        expect_that!(
            text("unknown", other(), blocker()),
            eq("Waits for \"Fix the build\", gone")
        );
    }

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
