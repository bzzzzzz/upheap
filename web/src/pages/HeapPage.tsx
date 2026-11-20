import { type NavigateOptions } from "@tanstack/react-router";
import { AnimatePresence } from "motion/react";
import { useEffect, useMemo, useState, type ReactNode } from "react";

import { ApiProblem, isNotFound, requestIdOf, type HeapRef } from "../api/client";
import type { Item, ItemPatchRequest, RankedItem, RankedList } from "../api/schema";
import { useUpdateItem } from "../api/queries/items";
import { useRankedList } from "../api/queries/lists";
import { useMe } from "../api/queries/me";
import { useConfirmAgentStateChange } from "../components/agentConfirm";
import { AddItem } from "../components/AddItem";
import {
  ComingRow,
  focusFirstRow,
  ItemRow,
  useRefocusStartedRow,
  type ChangeState,
  type RowFilters,
} from "../components/ItemRow";
import { STATE_LABELS } from "../components/labels";
import { ItemPane, useFullScreenShown } from "../components/ItemPane";
import { useListSearch } from "../components/listSearch";
import { itemProblem } from "../components/itemProblems";
import { FilterChips, ListControls, type Panel } from "../components/ListControls";
import { SelectionBars } from "../components/SelectionBar";
import { titleTeam } from "../components/teamName";
import { useToast } from "../components/UndoToast";
import { useCloseItem } from "../components/useCloseItem";
import { SelectButton, useSelectMode } from "../components/useSelectMode";
import {
  DEFAULT_MODES,
  filterCount,
  listQueryFor,
  matchesSearch,
  pinnedTitle,
  placeOf,
  SHOWN_STATES,
  splitList,
  toggled,
  viewChanges,
  type ListSearch,
} from "../components/listView";
import { HeapNotFound } from "../components/NotFound";
import { RequestId } from "../components/RequestId";

/**
 * The main view: what to work on, highest priority first. The server applies the view (`view`, from the URL); the
 * page only narrows it further by the search text.
 * `itemKey` is the key of the item open over the list, if any (ux.md, The item).
 */
export function HeapPage({
  heap: heapRef,
  itemKey,
  view,
  onViewChange,
}: {
  heap: HeapRef;
  itemKey: string | undefined;
  view: ListSearch;
  onViewChange: (view: ListSearch) => void;
}) {
  const { search, setSearch } = useListSearch();
  // While text is searched for, the reminders to come are asked for too, so the search finds them.
  const list = useRankedList(heapRef, listQueryFor(view, search));
  const [panel, setPanel] = useState<Panel>(null);
  const changeState = useChangeState(heapRef, list.data);
  // A change to an item an agent has asks first: every way of changing a state in the list goes through it.
  const agentConfirm = useConfirmAgentStateChange(changeState.run);
  const listPage = useMemo<NavigateOptions>(
    // Without full screen (ux.md, The item §23), which would carry on to the next item opened from the list.
    () => ({ to: "/teams/$team/heaps/$heap", params: heapRef, search: (prev) => ({ ...prev, full: undefined }) }),
    [heapRef],
  );
  const closeItem = useCloseItem(listPage, itemKey);
  const full = useFullScreenShown() && itemKey !== undefined;
  useRefocusStartedRow(list.data);

  // Select mode (ux.md, The list §19): the location's state says it is on, the selected items are kept here, and the
  // rows it can select are the ones of the list's sections that are shown.
  const { selecting, leave: leaveSelecting } = useSelectMode(heapRef);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  // The last row toggled, where a Shift-click's range starts.
  const [anchor, setAnchor] = useState<string | null>(null);
  const selectable = useMemo(
    () => (list.data ? everyEntry(shownSections(list.data, search)).map((entry) => entry.item) : []),
    [list.data, search],
  );
  const selectableIds = selectable.map((item) => item.id).join(",");
  // Adjusted as the page renders, not in an effect, so a render never shows what has been dropped. The selection ends
  // with select mode, and only what the list shows stays selected: an item a filter or the search hides doesn't come
  // back with it.
  const [seen, setSeen] = useState({ selecting, selectableIds });
  if (seen.selecting !== selecting || seen.selectableIds !== selectableIds) {
    setSeen({ selecting, selectableIds });
    const shown = new Set(selectableIds.split(","));
    const kept = selecting ? [...selected].filter((id) => shown.has(id)) : [];
    if (kept.length !== selected.size) setSelected(new Set(kept));
    if (!selecting && anchor !== null) setAnchor(null);
  }
  const toggle = (id: string, range: boolean) => {
    const from = anchor === null ? -1 : selectable.findIndex((item) => item.id === anchor);
    const to = selectable.findIndex((item) => item.id === id);
    const next = new Set(selected);
    if (range && from >= 0) {
      for (const item of selectable.slice(Math.min(from, to), Math.max(from, to) + 1)) next.add(item.id);
    } else if (!next.delete(id)) {
      next.add(id);
    }
    setSelected(next);
    setAnchor(id);
  };

  // The list's keys (ux.md, The list §4): the arrows start on the first row when nothing has the focus, and
  // Escape closes the open item, or leaves select mode. The rows handle the arrows, `x` and `s` themselves.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "ArrowDown" && document.activeElement === document.body) {
        event.preventDefault();
        focusFirstRow();
      } else if (event.key === "Escape" && selecting && !document.querySelector("[role=dialog]")) {
        event.preventDefault();
        leaveSelecting();
      } else if (event.key === "Escape" && closeItem && !document.querySelector("[role=dialog]")) {
        event.preventDefault();
        closeItem();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeItem, selecting, leaveSelecting]);

  const place = itemKey && list.data ? placeOf(list.data, itemKey) : undefined;
  // The rows on screen, top to bottom, the reminders to come last, for the pane's ↑ ↓.
  const onScreen = list.data ? rowsOnScreen(shownSections(list.data, search)) : [];
  const index = onScreen.findIndex((item) => item.key === itemKey);
  // A link to an item in a missing heap shows only the heap's not-found page.
  const pane = itemKey && closeItem && !(list.isError && isNotFound(list.error)) && (
    <ItemPane
      heapRef={heapRef}
      itemKey={itemKey}
      entry={place?.entry}
      section={place?.section}
      listed={index >= 0}
      previousKey={index > 0 ? onScreen[index - 1]?.key : undefined}
      nextKey={index >= 0 ? onScreen[index + 1]?.key : undefined}
      listName={list.data?.heap.name}
      onClose={closeItem}
    />
  );

  return (
    // On a wide screen the item pane sits beside the list; narrower, it's fixed over it.
    <div className="lg:flex lg:items-start">
      {/* Under a full-screen item the list stays, keeping its place, but out of reach of the keyboard. */}
      <div inert={full} className="min-w-0 flex-1 lg:has-[~[data-item-pane]]:pr-6">
        <ListPage
          heapRef={heapRef}
          itemKey={itemKey}
          list={list}
          view={view}
          onViewChange={onViewChange}
          search={search}
          onSearch={setSearch}
          panel={panel}
          onPanel={setPanel}
          onChangeState={agentConfirm.onChangeState}
          select={
            selecting
              ? { items: selectable, selected, onToggle: toggle, onSelect: setSelected, onDone: leaveSelecting }
              : undefined
          }
        />
      </div>
      {pane}
      {agentConfirm.confirm}
    </div>
  );
}

/**
 * Changes an item's state from the list, by a row's buttons, `x`, `s` or a swipe, then offers to undo it
 * (ux.md, The list §3). Undo puts back the state it had, and the blocker and the reason with it. After Done, when
 * nothing else is in progress, the toast also offers to start the top open item (ux.md, The list §10).
 */
export function useChangeState(heapRef: HeapRef, list: RankedList | undefined) {
  const update = useUpdateItem(heapRef);
  const setToast = useToast();
  const failed = (error: Error) =>
    setToast({ message: itemProblem(error), error: true, requestId: requestIdOf(error) });

  const run: ChangeState = (item, state, onError) => {
    const next = state === "done" && list ? nextToStart(list, item) : undefined;
    // `mutateAsync`, not `mutate` with callbacks: the toast can outlive the list (it stays while in the heap), and
    // TanStack drops a call's callbacks once the component that made it has unmounted.
    update.mutateAsync({ item, patch: { state } }).then(
      (saved) => {
        const restore: ItemPatchRequest = { state: item.state };
        if (item.state === "blocked" && item.blocked_by) restore.blocked_by_item_id = item.blocked_by.id;
        if (item.state === "blocked" && item.blocked_reason) restore.blocked_reason = item.blocked_reason;
        setToast({
          message: `“${item.title}” is ${STATE_LABELS[state].toLowerCase()}.`,
          undo: () => update.mutateAsync({ item: saved, patch: restore }).then(undefined, failed),
          action: next && { label: `Start next: ${next.title}`, run: () => run(next, "in_progress") },
        });
      },
      (error: Error) => {
        failed(error);
        onError?.();
      },
    );
  };
  return { run };
}

/**
 * The item to offer starting once `done` is done: the top open item, but only when no other item in the heap is in
 * progress, including any the view or its filters leave out. If one is, the user is carrying on with it, and the
 * offer would be noise.
 */
function nextToStart(list: RankedList, done: Item) {
  const othersInProgress = list.in_progress_count - (done.state === "in_progress" ? 1 : 0);
  if (othersInProgress > 0) return undefined;
  return everyEntry(list).find((entry) => entry.item.id !== done.id && entry.item.state === "open")?.item;
}

/** Select mode as the list uses it (ux.md, The list §19). */
type SelectState = {
  items: Item[];
  selected: ReadonlySet<string>;
  onToggle: (id: string, range: boolean) => void;
  onSelect: (ids: Set<string>) => void;
  onDone: () => void;
};

/** The list itself, with its controls. */
function ListPage({
  heapRef,
  itemKey,
  list,
  view,
  onViewChange,
  search,
  onSearch,
  panel,
  onPanel,
  onChangeState,
  select,
}: {
  heapRef: HeapRef;
  itemKey: string | undefined;
  list: ReturnType<typeof useRankedList>;
  view: ListSearch;
  onViewChange: (view: ListSearch) => void;
  search: string;
  onSearch: (search: string) => void;
  panel: Panel;
  onPanel: (panel: Panel) => void;
  onChangeState: ChangeState;
  /** Set in select mode: the rows that can be selected, those that are, and what the bars and rows do about it. */
  select: SelectState | undefined;
}) {
  const { data: me } = useMe();
  const team = titleTeam(me, heapRef);
  const filters = filterCount(view);
  const changed = filters > 0 || viewChanges(view).length > 0;
  if (list.isPending) {
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }

  const controls = (
    <ListControls
      heapRef={heapRef}
      view={view}
      onChange={onViewChange}
      search={search}
      onSearch={onSearch}
      panel={panel}
      onPanel={onPanel}
      extra={<SelectButton heapRef={heapRef} />}
      // None while the previous view stands in for this one, so the panel never shows its counts as this view's.
      counts={
        list.data && !list.isPlaceholderData
          ? { listed: everyEntry(list.data).length, total: list.data.unfiltered_count }
          : undefined
      }
    />
  );

  if (list.isError) {
    if (isNotFound(list.error)) return <HeapNotFound />;
    // A view the server refuses (a stale tag in a bookmark, say) keeps the controls, so it can be changed or reset.
    const badView = list.error instanceof ApiProblem && (list.error.status === 400 || list.error.status === 422);
    return (
      <main>
        {badView && controls}
        <div className="py-16 text-center">
          <p role="alert" className="text-red-700 dark:text-red-400">
            {badView ? `This view can't be shown: ${list.error.message}` : list.error.message}
          </p>
          <RequestId error={list.error} className="justify-center" />
          {badView && (
            <button
              type="button"
              onClick={() => onViewChange({})}
              className="mt-4 text-sm text-stone-500 underline hover:text-stone-900 dark:hover:text-stone-100"
            >
              Reset to the default view
            </button>
          )}
        </div>
      </main>
    );
  }

  const { heap, hidden_count, unfiltered_count } = list.data;
  const { pinned, ranked, blocked, coming } = shownSections(list.data, search);
  const listed = everyEntry(list.data).length;
  const shown = pinned.length + ranked.length + blocked.length + coming.length;
  const narrowed = search.trim() !== "" || filters > 0;
  // A tag or parameter chip in a row joins the view's filter, or leaves it when it is in it already.
  const rowFilters: RowFilters = {
    tags: new Set(splitList(view.tag)),
    options: new Set(splitList(view.param)),
    toggleTag: (tagId) => onViewChange({ ...view, tag: toggled(view.tag, tagId) }),
    toggleOption: (optionId) => onViewChange({ ...view, param: toggled(view.param, optionId) }),
  };
  const rows = (entries: RankedItem[], pinned?: boolean) => (
    <ItemList
      entries={entries}
      filters={rowFilters}
      heapRef={heapRef}
      itemKey={itemKey}
      onChangeState={onChangeState}
      compact={(entry) => pinned === true && entry.item.state === "in_progress"}
      select={select}
    />
  );

  return (
    <main>
      <div className="flex items-center justify-between gap-4 sm:mt-6 sm:mb-4">
        {/* On a phone the heap name is already at the top, as the heap switcher. */}
        <h1 className="min-w-0 truncate text-xl font-semibold tracking-tight max-sm:sr-only">
          {heap.name}
          {team && <span className="font-normal text-muted"> · {team}</span>}
        </h1>
        <AddItem heapRef={heapRef} view={view} search={search} disabled={heap.archived} />
      </div>

      {select ? (
        <>
          {/* The wide bar takes the toolbar's place; a phone keeps the toolbar under its own bars. */}
          <div className="sm:hidden">{controls}</div>
          <SelectionBars
            heapRef={heapRef}
            items={select.items}
            selected={select.selected}
            onSelect={select.onSelect}
            onDone={select.onDone}
          />
        </>
      ) : (
        controls
      )}

      {changed && (
        <FilterChips
          heapRef={heapRef}
          view={view}
          onChange={onViewChange}
          // What the filters leave, before the search narrows it further, so both counts mean the same thing.
          shown={listed}
          // Not while the previous view stands in for this one, so a count from it never shows ("5 of 3").
          total={list.isPlaceholderData ? undefined : unfiltered_count}
        />
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
            {listed === 0 && hidden_count === 0 && !narrowed
              ? "Nothing here yet. Add the first thing on your plate."
              : "No items match."}
          </p>
        )}

        {pinned.length > 0 && <Section title={pinnedTitle(pinned)}>{rows(pinned, true)}</Section>}

        {ranked.length > 0 && <Section title={pinned.length > 0 ? "Up next" : undefined}>{rows(ranked)}</Section>}

        {blocked.length > 0 && <Section title="Blocked">{rows(blocked)}</Section>}

        {coming.length > 0 && (
          <Section title="Reminders to come">
            <ol className="flex flex-col">
              {coming.map((item) => (
                <ComingRow
                  key={item.id}
                  item={item}
                  heapRef={heapRef}
                  selected={item.key === itemKey}
                  itemOpen={itemKey !== undefined}
                  filters={rowFilters}
                />
              ))}
            </ol>
          </Section>
        )}
      </div>

      {hidden_count > 0 && <HiddenNote count={hidden_count} view={view} onViewChange={onViewChange} />}
    </main>
  );
}

/**
 * The list's groups, narrowed by the search text. `coming` is the reminders to come the server sent: all of them when
 * the view shows them, the ones searched for otherwise.
 */
function shownSections(list: RankedList, search: string) {
  const matching = (group: RankedItem[]) => group.filter((entry) => matchesSearch(entry, search));
  return {
    pinned: matching(list.pinned),
    ranked: matching(list.ranked),
    blocked: matching(list.blocked),
    coming: list.reminders.filter((item) => matchesSearch({ item }, search)),
  };
}

/** The items of the sections shown, in the order the rows are on screen. */
const rowsOnScreen = (sections: ReturnType<typeof shownSections>) => [
  ...everyEntry(sections).map((entry) => entry.item),
  ...sections.coming,
];

const everyEntry = (list: Pick<RankedList, "pinned" | "ranked" | "blocked">) => [
  ...list.pinned,
  ...list.ranked,
  ...list.blocked,
];

/**
 * How many items the view hides by their state, with the fix (ux.md, The list §12): Show them puts every
 * hidden state back in the list, at its default (In progress pinned, Open shown), or shown for Blocked, which is
 * hidden by default. Under the default view that's the blocked items, and the note says so.
 */
export function HiddenNote({
  count,
  view,
  onViewChange,
}: {
  count: number;
  view: ListSearch;
  onViewChange: (view: ListSearch) => void;
}) {
  const hidden = SHOWN_STATES.filter((state) => (view[state] ?? DEFAULT_MODES[state]) === "hidden");
  const kind = hidden.length === 1 && hidden[0] === "blocked" ? "blocked " : "";
  const show = () =>
    onViewChange({
      ...view,
      ...Object.fromEntries(hidden.map((state) => [state, DEFAULT_MODES[state] === "hidden" ? "shown" : undefined])),
    });
  return (
    <p className="mt-6 text-center text-sm text-muted">
      {count === 1 ? `1 ${kind}item is` : `${count} ${kind}items are`} hidden.{" "}
      <button type="button" onClick={show} className="underline hover:text-ink">
        {count === 1 ? "Show it" : "Show them"}
      </button>
    </p>
  );
}

export function Section({ title, children }: { title: string | undefined; children: ReactNode }) {
  return (
    <section aria-label={title} className="mt-6">
      {title && <h2 className="mb-1 px-2 text-xs font-semibold tracking-wide text-muted uppercase">{title}</h2>}
      {children}
    </section>
  );
}

/** With `compact`, a row leaves out why it ranks where it does, so pinned in-progress rows don't crowd the list. */
function ItemList({
  entries,
  heapRef,
  itemKey,
  filters,
  onChangeState,
  compact,
  select,
}: {
  entries: RankedItem[];
  heapRef: HeapRef;
  itemKey: string | undefined;
  filters: RowFilters;
  onChangeState: ChangeState;
  compact: (entry: RankedItem) => boolean;
  select: SelectState | undefined;
}) {
  return (
    <ol className="flex flex-col">
      <AnimatePresence initial={false}>
        {entries.map((entry) => (
          <ItemRow
            key={entry.item.id}
            entry={entry}
            heapRef={heapRef}
            selected={entry.item.key === itemKey}
            itemOpen={itemKey !== undefined}
            compact={compact(entry)}
            filters={filters}
            onChangeState={onChangeState}
            selection={
              select && {
                selected: select.selected.has(entry.item.id),
                onToggle: (range) => select.onToggle(entry.item.id, range),
              }
            }
          />
        ))}
      </AnimatePresence>
    </ol>
  );
}
