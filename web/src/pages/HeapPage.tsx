import { AnimatePresence } from "motion/react";
import { useState } from "react";

import type { RankedItem } from "../api/client";
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
          <ItemList entries={pinned} heapId={heapId} compact />
        </Section>
      )}

      {ranked.length > 0 && (
        <Section title={pinned.length > 0 ? "Up next" : undefined}>
          <ItemList entries={ranked} heapId={heapId} />
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
          {showBlocked && <ItemList entries={blocked} heapId={heapId} />}
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

function ItemList({ entries, heapId, compact }: { entries: RankedItem[]; heapId: string; compact?: boolean }) {
  return (
    <ol className="rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900">
      <AnimatePresence initial={false}>
        {entries.map((entry) => (
          <ItemRow key={entry.item.id} entry={entry} heapId={heapId} compact={compact} />
        ))}
      </AnimatePresence>
    </ol>
  );
}
