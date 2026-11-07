import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { AnimatePresence } from "motion/react";
import { useCallback, useEffect, useState, type ReactNode } from "react";

import { ApiProblem, type ItemPatch, type RankedItem, type RankedList, type HeapRef } from "../api/client";
import { useRankedList, useUpdateItem } from "../api/queries";
import { AddItem } from "../components/AddItem";
import { CopyLink } from "../components/CopyLink";
import { focusFirstRow, focusRowOf, ItemRow, type ChangeState } from "../components/ItemRow";
import { STATE_LABELS } from "../components/labels";
import { ItemPane } from "../components/ItemPane";
import { itemProblem } from "../components/itemProblems";
import { ListControls, type Panel } from "../components/ListControls";
import { SavedViewsBar } from "../components/SavedViewsBar";
import { UndoToast, type Toast } from "../components/UndoToast";
import { filterCount, matchesSearch, toListQuery, type ListSearch } from "../components/listView";

/**
 * The main view: what to work on, highest priority first. The server applies the view (`view`, from the URL); the
 * page only narrows it further by the search text. Saved views are links that put their settings in the URL.
 * `itemKey` is the key of the item open over the list, if any (app-ux.md, The item).
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
  const list = useRankedList(heapRef, toListQuery(view));
  const [search, setSearch] = useState("");
  const [panel, setPanel] = useState<Panel>(null);
  // Bumped to move focus into the open panel, for "Change view" below the list.
  const [panelFocus, setPanelFocus] = useState(0);
  const changeState = useChangeState(heapRef);
  const closeItem = useCloseItem(heapRef, itemKey);

  // The list's keys (app-ux.md, The list §4): the arrows start on the first row when nothing has the focus, and
  // Escape closes the open item. The rows handle the arrows and `x` themselves.
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

  const place = itemKey && list.data ? placeOf(list.data, itemKey) : undefined;
  const pane = itemKey && closeItem && (
    <ItemPane
      key={itemKey}
      heapRef={heapRef}
      itemKey={itemKey}
      entry={place?.entry}
      section={place?.section}
      onClose={closeItem}
    />
  );

  return (
    // On a wide screen the item pane sits beside the list; narrower, it's fixed over it.
    <div className="lg:flex lg:items-start">
      <div className="min-w-0 flex-1 lg:has-[~[data-item-pane]]:pr-6">
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
          panelFocus={panelFocus}
          onPanelFocus={() => setPanelFocus((n) => n + 1)}
          onChangeState={changeState.run}
        />
      </div>
      {pane}
      <UndoToast toast={changeState.toast} onDismiss={changeState.dismiss} />
    </div>
  );
}

/** Where an item sits in the list, for the pane's "#3 · Up next". */
function placeOf(list: RankedList, itemKey: string): { entry: RankedItem; section: string | undefined } | undefined {
  const sections: [RankedItem[], string | undefined][] = [
    [list.pinned, pinnedTitle(list.pinned)],
    [list.ranked, list.pinned.length > 0 ? "Up next" : undefined],
    [list.blocked, "Blocked"],
  ];
  for (const [entries, section] of sections) {
    const entry = entries.find((e) => e.item.key === itemKey);
    if (entry) return { entry, section };
  }
  return undefined;
}

const pinnedTitle = (pinned: RankedItem[]) =>
  pinned.every((entry) => entry.item.state === "in_progress") ? "In progress" : "Pinned";

/**
 * Changes an item's state from the list, by its circle, `x` or a swipe, then offers to undo it (app-ux.md, The list
 * §3). Undo puts back the state it had, and the blocker with it.
 */
function useChangeState(heapRef: HeapRef) {
  const update = useUpdateItem(heapRef);
  const [toast, setToast] = useState<Toast | null>(null);
  const dismiss = useCallback(() => setToast(null), []);
  const failed = (error: Error) => setToast({ id: Date.now(), message: itemProblem(error), error: true });

  const run: ChangeState = (item, state, onError) =>
    update.mutate(
      { item, patch: { state } },
      {
        onSuccess: (saved) => {
          const restore: ItemPatch = { state: item.state };
          if (item.state === "blocked" && item.blocked_by) restore.blocked_by_item_id = item.blocked_by.id;
          setToast({
            id: Date.now(),
            message: `“${item.title}” is ${STATE_LABELS[state].toLowerCase()}.`,
            undo: () => update.mutate({ item: saved, patch: restore }, { onError: failed }),
          });
        },
        onError: (error) => {
          failed(error);
          onError?.();
        },
      },
    );
  return { run, toast, dismiss };
}

/**
 * Closes the open item. Back closes it when the app opened it, so the history doesn't gain an entry per look at an
 * item; opened from outside (a link, a reload), it goes to the list instead. Focus goes back to the item's row.
 */
function useCloseItem(heapRef: HeapRef, itemKey: string | undefined) {
  const router = useRouter();
  const navigate = useNavigate();
  const closesBack = useRouterState({ select: (state) => state.location.state.closesBack === true });
  const close = useCallback(() => {
    if (itemKey === undefined) return;
    if (closesBack) {
      router.history.back();
    } else {
      void navigate({ to: "/teams/$team/heaps/$heap", params: heapRef, search: true });
    }
    window.setTimeout(() => focusRowOf(itemKey));
  }, [router, navigate, closesBack, heapRef, itemKey]);
  return itemKey === undefined ? undefined : close;
}

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
  panelFocus,
  onPanelFocus,
  onChangeState,
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
  panelFocus: number;
  onPanelFocus: () => void;
  onChangeState: ChangeState;
}) {
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
      panelFocus={panelFocus}
    />
  );

  const savedViews = <SavedViewsBar heapRef={heapRef} view={view} onViewChange={onViewChange} />;

  if (list.isError) {
    // A view the server refuses (a stale tag in a bookmark, say) keeps the controls, so it can be changed or reset.
    const badView = list.error instanceof ApiProblem && (list.error.status === 400 || list.error.status === 422);
    return (
      <main>
        {badView && savedViews}
        {badView && controls}
        <div className="py-16 text-center">
          <p role="alert" className="text-red-700 dark:text-red-400">
            {badView ? `This view can't be shown: ${list.error.message}` : list.error.message}
          </p>
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

  const { heap, hidden_count } = list.data;
  const matching = (group: RankedItem[]) => group.filter((entry) => matchesSearch(entry, search));
  const pinned = matching(list.data.pinned);
  const ranked = matching(list.data.ranked);
  const blocked = matching(list.data.blocked);
  const listed = everyEntry(list.data).length;
  const shown = pinned.length + ranked.length + blocked.length;
  const narrowed = search.trim() !== "" || filterCount(view) > 0;
  const rows = (entries: RankedItem[], pinned?: boolean) => (
    <ItemList
      entries={entries}
      heapRef={heapRef}
      itemKey={itemKey}
      onChangeState={onChangeState}
      compact={(entry) => pinned === true && entry.item.state === "in_progress"}
    />
  );

  return (
    <main>
      <div className="mt-2 flex items-baseline justify-end sm:mt-6 sm:justify-between">
        {/* On a phone the heap name is already at the top, as the heap switcher. */}
        <h1 className="text-xl font-semibold tracking-tight max-sm:sr-only">{heap.name}</h1>
        <div className="flex items-baseline gap-3">
          <CopyLink handles={[heapRef.team, heapRef.heap]} />
          <button
            type="button"
            onClick={() => void list.refresh()}
            className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
            title="Re-rank at the current time"
          >
            Refresh order
          </button>
        </div>
      </div>

      {savedViews}

      <AddItem heapRef={heapRef} disabled={heap.archived} />

      {controls}

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
      </div>

      {hidden_count > 0 && (
        <p className="mt-6 text-center text-sm text-stone-500">
          {hidden_count === 1 ? "1 item is" : `${hidden_count} items are`} hidden by the view settings.{" "}
          <button
            type="button"
            onClick={() => {
              onPanel("view");
              onPanelFocus();
            }}
            className="underline hover:text-stone-900 dark:hover:text-stone-100"
          >
            Change view
          </button>
        </p>
      )}
    </main>
  );
}

const everyEntry = (list: RankedList) => [...list.pinned, ...list.ranked, ...list.blocked];

function Section({ title, children }: { title: string | undefined; children: ReactNode }) {
  return (
    <section className="mt-6">
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
  onChangeState,
  compact,
}: {
  entries: RankedItem[];
  heapRef: HeapRef;
  itemKey: string | undefined;
  onChangeState: ChangeState;
  compact: (entry: RankedItem) => boolean;
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
            onChangeState={onChangeState}
          />
        ))}
      </AnimatePresence>
    </ol>
  );
}
