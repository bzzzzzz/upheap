import { type NavigateOptions } from "@tanstack/react-router";
import { AnimatePresence } from "motion/react";
import { useEffect, useMemo, useState } from "react";

import type { HeapRef } from "../api/client";
import type { AcrossHeapsItem, AcrossHeapsList, RankedList } from "../api/schema";
import { useAcrossHeapsList, useMe } from "../api/queries";
import { useConfirmAgentStateChange } from "../components/agentConfirm";
import { AddItem } from "../components/AddItem";
import { focusFirstRow, ItemRow, useRefocusStartedRow } from "../components/ItemRow";
import { ItemPane } from "../components/ItemPane";
import { FilterChips, ListControls, type Panel, type TeamScope } from "../components/ListControls";
import { useListSearch } from "../components/listSearch";
import {
  filterCount,
  itemAddress,
  matchesSearch,
  parseItemAddress,
  pinnedTitle,
  toListQuery,
  viewChanges,
  type AcrossSearch,
} from "../components/listView";
import { teamName, titleTeam } from "../components/teamName";
import { useCloseItem } from "../components/useCloseItem";
import { keptHeap } from "./AppLayout";
import { HiddenNote, Section, useChangeState } from "./HeapPage";

/** The list the open item closes to: this page, without the item. */
const ALL_ITEMS: NavigateOptions = { to: "/all", search: (prev) => ({ ...prev, item: undefined }) };

/** What Add is for while the person has no heap: it is disabled then, and never opens. */
const NO_HEAP: HeapRef = { team: "", heap: "" };

const heapOf = (row: AcrossHeapsItem): HeapRef => ({ team: row.heap.team_slug, heap: row.heap.slug });
const addressOf = (row: AcrossHeapsItem) => itemAddress(heapOf(row), row.entry.item.key);

/**
 * Every open item of all the person's heaps in one list, most important first (ux.md, All items). The rows are the
 * heap list's, each with its heap; the item opens over the list, from its own heap. The server applies the view (`view`,
 * from the URL); the page only narrows it further by the search text.
 */
export function AllItemsPage({
  view,
  onViewChange,
}: {
  view: AcrossSearch;
  onViewChange: (view: AcrossSearch) => void;
}) {
  const { data: me } = useMe();
  const { search, setSearch } = useListSearch();
  const list = useAcrossHeapsList({ ...toListQuery(view), team: view.team });
  const [panel, setPanel] = useState<Panel>(null);
  const open = parseItemAddress(view.item);
  const closeItem = useCloseItem(ALL_ITEMS, open?.key);
  // The rows' entries, as the heap list's are, for the refocus after a row is started.
  const entries = useMemo<Pick<RankedList, "pinned" | "ranked" | "blocked"> | undefined>(
    () =>
      list.data && {
        pinned: list.data.pinned.map((row) => row.entry),
        ranked: list.data.ranked.map((row) => row.entry),
        blocked: list.data.blocked.map((row) => row.entry),
      },
    [list.data],
  );
  useRefocusStartedRow(entries);

  // The list's keys (ux.md, The list §4): the arrows start on the first row when nothing has the focus, and
  // Escape closes the open item. The rows handle the arrows, `x` and `s` themselves.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "ArrowDown" && document.activeElement === document.body) {
        event.preventDefault();
        focusFirstRow();
      } else if (event.key === "Escape" && closeItem && !document.querySelector("[role=dialog]")) {
        event.preventDefault();
        closeItem();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeItem]);

  if (!me) return null;
  const filters = filterCount(view);
  const chosen = me.teams.find((t) => t.slug === view.team);
  // A link to a team the person isn't in (or has left): the server answers not found, and the page says why.
  const unknownTeam = view.team !== undefined && chosen === undefined;
  const changed = filters > 0 || viewChanges(view).length > 0 || chosen !== undefined;
  const kept = keptHeap(me, view.team);
  const keptArchived = me.teams.find((t) => t.slug === kept?.team)?.heaps.find((h) => h.slug === kept?.heap)?.archived;
  // A person belongs to one team or several; the team's name helps only when there are several.
  const manyTeams = me.teams.length > 1;
  const team: TeamScope = {
    me,
    chosen: view.team,
    onChange: (slug) => onViewChange({ ...view, team: slug }),
  };
  const heapLabel = (row: AcrossHeapsItem) => {
    const team = manyTeams && !chosen ? titleTeam(me, heapOf(row)) : undefined;
    return team ? `${row.heap.name} · ${team}` : row.heap.name;
  };

  const data = list.data;
  const sections = data && shownSections(data, search);
  const onScreen = sections ? [...sections.pinned, ...sections.ranked, ...sections.blocked] : [];
  const index = open ? onScreen.findIndex((row) => addressOf(row) === view.item) : -1;
  const previous = index > 0 ? onScreen[index - 1] : undefined;
  const next = index >= 0 ? onScreen[index + 1] : undefined;
  const place = open && data ? placeOf(data, view.item) : undefined;
  const pane = open && closeItem && (
    <ItemPane
      heapRef={{ team: open.team, heap: open.heap }}
      itemKey={open.key}
      entry={place && { ...place.row.entry, position: place.row.position }}
      section={place?.section}
      listed={index >= 0}
      previousKey={previous && addressOf(previous)}
      nextKey={next && addressOf(next)}
      walkLink={(item) => ({ to: "/all", search: (prev) => ({ ...prev, item }) })}
      onClose={closeItem}
    />
  );

  const listed = data ? data.pinned.length + data.ranked.length + data.blocked.length : 0;
  const shown = sections ? sections.pinned.length + sections.ranked.length + sections.blocked.length : 0;
  const narrowed = search.trim() !== "" || filters > 0;
  const rows = (group: AcrossHeapsItem[], pinned?: boolean) => (
    <ol className="flex flex-col">
      <AnimatePresence initial={false}>
        {group.map((row) => (
          <AcrossRow
            key={row.entry.item.id}
            row={row}
            selected={addressOf(row) === view.item}
            itemOpen={open !== undefined}
            compact={pinned === true && row.entry.item.state === "in_progress"}
            heapLabel={heapLabel(row)}
          />
        ))}
      </AnimatePresence>
    </ol>
  );

  return (
    // On a wide screen the item pane sits beside the list; narrower, it's fixed over it.
    <div className="lg:flex lg:items-start">
      <div className="min-w-0 flex-1 lg:has-[~[data-item-pane]]:pr-6">
        <main>
          <div className="mt-6 flex items-center justify-between gap-4 sm:mb-4">
            <div className="flex min-w-0 flex-col gap-1">
              <h1 className="text-[22px] font-semibold tracking-[-0.015em]">
                All items{data && data.count > 0 && ` · ${data.count}`}
              </h1>
              {!unknownTeam && (
                <p className="text-[13px] text-ink-soft max-sm:text-sm">
                  {chosen
                    ? `${chosen.personal ? "Your personal team" : teamName(chosen, me)}'s heaps, most important first.`
                    : "All your heaps, most important first."}
                </p>
              )}
            </div>
            <AddItem
              heapRef={kept ?? NO_HEAP}
              view={view}
              search={search}
              acrossHeaps
              disabled={kept === undefined || keptArchived === true}
            />
          </div>

          {list.isPending ? (
            <p className="py-16 text-center text-muted">Loading…</p>
          ) : !data || !sections ? (
            <p role="alert" className="py-16 text-center text-red-700 dark:text-red-400">
              {unknownTeam ? "You're not in this team." : list.error?.message}
            </p>
          ) : (
            <>
              <ListControls
                view={view}
                onChange={onViewChange}
                search={search}
                onSearch={setSearch}
                panel={panel}
                onPanel={setPanel}
                // None while the previous view stands in for this one, so the panel never shows its counts as this view's.
                counts={list.isPlaceholderData ? undefined : { listed: data.count, total: undefined }}
                team={team}
              />

              {changed && (
                <FilterChips view={view} onChange={onViewChange} shown={data.count} total={undefined} team={team} />
              )}

              {search.trim() !== "" && (
                <p role="status" className="sr-only">
                  {shown === 1 ? "1 item matches" : `${shown} items match`}
                </p>
              )}

              {/* While another view loads, the previous one stays on screen, dimmed. */}
              <div aria-busy={list.isPlaceholderData} className={list.isPlaceholderData ? "opacity-60" : undefined}>
                {shown === 0 && (
                  <p className="py-12 text-center text-stone-500">
                    {listed === 0 && data.hidden_count === 0 && !narrowed
                      ? "Nothing open in your heaps."
                      : "No items match."}
                  </p>
                )}

                {sections.pinned.length > 0 && (
                  <Section title={pinnedTitle(sections.pinned.map((row) => row.entry))}>
                    {rows(sections.pinned, true)}
                  </Section>
                )}

                {sections.ranked.length > 0 && (
                  <Section title={sections.pinned.length > 0 ? "Up next" : undefined}>{rows(sections.ranked)}</Section>
                )}

                {sections.blocked.length > 0 && <Section title="Blocked">{rows(sections.blocked)}</Section>}
              </div>

              {data.hidden_count > 0 && (
                <HiddenNote count={data.hidden_count} view={view} onViewChange={onViewChange} />
              )}

              {data.count > listed && (
                <p className="mt-6 text-[13px] text-muted">
                  Showing the {listed} most important of {data.count}.
                </p>
              )}
            </>
          )}
        </main>
      </div>
      {pane}
    </div>
  );
}

/**
 * One row of the list across heaps. It changes the item's state in the item's own heap, so each row has its own
 * change, and the question to take the item back from an agent. No offer to start the next item after Done: the
 * heap's list isn't in view to say which it is.
 */
function AcrossRow({
  row,
  selected,
  itemOpen,
  compact,
  heapLabel,
}: {
  row: AcrossHeapsItem;
  selected: boolean;
  itemOpen: boolean;
  compact: boolean;
  heapLabel: string;
}) {
  const heapRef = heapOf(row);
  const changeState = useChangeState(heapRef, undefined);
  const agentConfirm = useConfirmAgentStateChange(changeState.run);
  const item = addressOf(row);
  return (
    <>
      <ItemRow
        entry={row.entry}
        heapRef={heapRef}
        selected={selected}
        itemOpen={itemOpen}
        compact={compact}
        heapLabel={heapLabel}
        link={{ to: "/all", search: (prev) => ({ ...prev, item }) }}
        onChangeState={agentConfirm.onChangeState}
      />
      {agentConfirm.confirm}
    </>
  );
}

/** The list's groups, narrowed by the search text. */
function shownSections(list: AcrossHeapsList, search: string) {
  const matching = (group: AcrossHeapsItem[]) => group.filter((row) => matchesSearch(row.entry, search));
  return { pinned: matching(list.pinned), ranked: matching(list.ranked), blocked: matching(list.blocked) };
}

/** The row of the item at `address` in the list as the server ranked it, and its group's title, if it shows one. */
function placeOf(list: AcrossHeapsList, address: string | undefined) {
  const groups: [AcrossHeapsItem[], string | undefined][] = [
    [list.pinned, pinnedTitle(list.pinned.map((row) => row.entry))],
    [list.ranked, list.pinned.length > 0 ? "Up next" : undefined],
    [list.blocked, "Blocked"],
  ];
  for (const [rows, section] of groups) {
    const row = rows.find((candidate) => addressOf(candidate) === address);
    if (row) return { row, section };
  }
  return undefined;
}
