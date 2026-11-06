import { AnimatePresence } from "motion/react";
import { useState } from "react";

import type { ItemRef, RankedItem } from "../api/client";
import { useRankedList } from "../api/queries";
import { AddItemForm } from "../components/AddItemForm";
import { ItemRow } from "../components/ItemRow";

/** The main view: what to work on, highest priority first. */
export function HeapPage({ heapId }: { heapId: string }) {
  const list = useRankedList(heapId);
  const [showBlocked, setShowBlocked] = useState(false);

  if (list.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }
  if (list.isError) {
    return (
      <p role="alert" className="py-16 text-center text-red-700 dark:text-red-400">
        {list.error.message}
      </p>
    );
  }

  const { heap, pinned, ranked, blocked } = list.data;
  const empty = pinned.length + ranked.length + blocked.length === 0;
  const blockerCandidates = blockerCandidatesFor([...pinned, ...ranked, ...blocked]);

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

      {empty && (
        <p className="py-12 text-center text-stone-500">Nothing here yet. Add the first thing on your plate.</p>
      )}

      {pinned.length > 0 && (
        <Section title="In progress">
          <ItemList entries={pinned} heapId={heapId} blockerCandidates={blockerCandidates} compact />
        </Section>
      )}

      {ranked.length > 0 && (
        <Section title={pinned.length > 0 ? "Up next" : undefined}>
          <ItemList entries={ranked} heapId={heapId} blockerCandidates={blockerCandidates} />
        </Section>
      )}

      {blocked.length > 0 && (
        <section className="mt-8">
          <button
            type="button"
            onClick={() => setShowBlocked(!showBlocked)}
            aria-expanded={showBlocked}
            className="text-sm font-medium text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
          >
            {showBlocked ? "Hide" : "Show"} blocked ({blocked.length})
          </button>
          {showBlocked && <ItemList entries={blocked} heapId={heapId} blockerCandidates={blockerCandidates} />}
        </section>
      )}
    </main>
  );
}

function Section({ title, children }: { title: string | undefined; children: React.ReactNode }) {
  return (
    <section className="mt-6">
      {title && <h2 className="mb-1 text-xs font-semibold tracking-wide text-stone-500 uppercase">{title}</h2>}
      {children}
    </section>
  );
}

function ItemList({
  entries,
  heapId,
  blockerCandidates,
  compact,
}: {
  entries: RankedItem[];
  heapId: string;
  blockerCandidates: (itemId: string) => ItemRef[];
  compact?: boolean;
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
            compact={compact}
          />
        ))}
      </AnimatePresence>
    </ol>
  );
}

/**
 * The items each item may wait for: every listed item except itself and those that already wait for it, directly
 * or down a chain, which the server would refuse as a cycle.
 */
function blockerCandidatesFor(entries: RankedItem[]): (itemId: string) => ItemRef[] {
  const waitsFor = new Map(entries.map(({ item }) => [item.id, item.blocked_by?.id]));
  const refs: ItemRef[] = entries.map(({ item }) => ({ id: item.id, title: item.title, state: item.state }));
  const reaches = (from: string, target: string) => {
    const seen = new Set<string>();
    for (let at: string | undefined = from; at !== undefined && !seen.has(at); at = waitsFor.get(at)) {
      if (at === target) return true;
      seen.add(at);
    }
    return false;
  };
  return (itemId) => refs.filter((ref) => !reaches(ref.id, itemId));
}
