import { AnimatePresence } from "motion/react";
import { useState } from "react";

import { ApiProblem, type ItemRef, type RankedItem, type RankedList } from "../api/client";
import { useBlockerCandidates, useRankedList } from "../api/queries";
import { AddItemForm } from "../components/AddItemForm";
import { blockerCandidatesFor } from "../components/blockerCandidates";
import { ItemRow } from "../components/ItemRow";
import { ListControls, type Panel } from "../components/ListControls";
import { filterCount, matchesSearch, toListQuery, type ListSearch } from "../components/listView";

/**
 * The main view: what to work on, highest priority first. The server applies the view (`view`, from the URL); the
 * page only narrows it further by the search text.
 */
export function HeapPage({
  heapId,
  view,
  onViewChange,
}: {
  heapId: string;
  view: ListSearch;
  onViewChange: (view: ListSearch) => void;
}) {
  const list = useRankedList(heapId, toListQuery(view));
  const [search, setSearch] = useState("");
  const [panel, setPanel] = useState<Panel>(null);
  // Bumped to move focus into the open panel, for "Change view" below the list.
  const [panelFocus, setPanelFocus] = useState(0);
  // Only blocked rows pick a blocker, so the candidates load only when the view shows one.
  const listsBlocked = list.data ? everyEntry(list.data).some((entry) => entry.item.state === "blocked") : false;
  const candidates = useBlockerCandidates(heapId, listsBlocked);

  if (list.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }

  const controls = (
    <ListControls
      heapId={heapId}
      view={view}
      onChange={onViewChange}
      search={search}
      onSearch={setSearch}
      panel={panel}
      onPanel={setPanel}
      panelFocus={panelFocus}
    />
  );

  if (list.isError) {
    // A view the server refuses (a stale tag in a bookmark, say) keeps the controls, so it can be changed or reset.
    const badView = list.error instanceof ApiProblem && (list.error.status === 400 || list.error.status === 422);
    return (
      <main>
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
  // A blocked item may wait for any non-closed item, including ones this view leaves out.
  const blockerCandidates = blockerCandidatesFor(candidates.data ?? []);

  return (
    <main>
      <div className="mt-6 flex items-baseline justify-between">
        <h1 className="text-xl font-semibold tracking-tight">
          {heap.name}
          {heap.archived && <span className="ml-2 text-sm font-normal text-stone-500">archived, read-only</span>}
        </h1>
        <button
          type="button"
          onClick={() => void list.refresh()}
          className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
          title="Re-rank at the current time"
        >
          Refresh order
        </button>
      </div>

      <AddItemForm heapId={heapId} disabled={heap.archived} />

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

        {pinned.length > 0 && (
          <Section title={pinned.every((entry) => entry.item.state === "in_progress") ? "In progress" : "Pinned"}>
            <ItemList entries={pinned} heapId={heapId} blockerCandidates={blockerCandidates} pinned />
          </Section>
        )}

        {ranked.length > 0 && (
          <Section title={pinned.length > 0 ? "Up next" : undefined}>
            <ItemList entries={ranked} heapId={heapId} blockerCandidates={blockerCandidates} />
          </Section>
        )}

        {blocked.length > 0 && (
          <Section title="Blocked">
            <ItemList entries={blocked} heapId={heapId} blockerCandidates={blockerCandidates} />
          </Section>
        )}
      </div>

      {hidden_count > 0 && (
        <p className="mt-6 text-center text-sm text-stone-500">
          {hidden_count === 1 ? "1 item is" : `${hidden_count} items are`} hidden by the view settings.{" "}
          <button
            type="button"
            onClick={() => {
              setPanel("view");
              setPanelFocus((n) => n + 1);
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

function Section({ title, children }: { title: string | undefined; children: React.ReactNode }) {
  return (
    <section className="mt-6">
      {title && <h2 className="mb-1 text-xs font-semibold tracking-wide text-stone-500 uppercase">{title}</h2>}
      {children}
    </section>
  );
}

/** With `pinned`, in-progress rows are compact so they don't crowd the list; other pinned rows stay full. */
function ItemList({
  entries,
  heapId,
  blockerCandidates,
  pinned,
}: {
  entries: RankedItem[];
  heapId: string;
  blockerCandidates: (itemId: string) => ItemRef[];
  pinned?: boolean;
}) {
  return (
    <ol className="rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900">
      <AnimatePresence initial={false}>
        {entries.map((entry) => (
          <ItemRow
            key={entry.item.id}
            entry={entry}
            heapId={heapId}
            blockerCandidates={blockerCandidates(entry.item.id)}
            compact={pinned === true && entry.item.state === "in_progress"}
          />
        ))}
      </AnimatePresence>
    </ol>
  );
}
