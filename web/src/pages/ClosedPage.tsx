import { Link } from "@tanstack/react-router";
import { useId } from "react";

import type { ClosedGroup, HeapRef, Item } from "../api/client";
import { useClosedItems, useReopenItem } from "../api/queries";
import { becameWords, CLOSED_GROUP_LABELS, STATE_LABELS } from "../components/labels";
import { useUserName } from "../components/userName";

/** A run of items next to each other with the same `closed_group`, as the server groups them. */
interface ClosedSection {
  group: ClosedGroup;
  items: Item[];
}

/**
 * Splits `items`, in server order, into runs that share a `closed_group`. Every page loaded so far is one list, so a
 * group that goes on into the next page stays one section.
 */
function toSections(items: Item[]): ClosedSection[] {
  const sections: ClosedSection[] = [];
  for (const item of items) {
    // Closed items always have a group; the fallback only satisfies the type.
    const group = item.closed_group ?? "earlier";
    const last = sections.at(-1);
    if (last && last.group === group) {
      last.items.push(item);
    } else {
      sections.push({ group, items: [item] });
    }
  }
  return sections;
}

/**
 * Done and won't-do items, most recently closed first (ux.md, Closed), under "Today", "This week" and
 * "Earlier". Each row says who closed it, and offers Reopen while the server allows it.
 */
export function ClosedPage({ heap: heapRef }: { heap: HeapRef }) {
  const closed = useClosedItems(heapRef);
  const reopen = useReopenItem(heapRef);

  if (closed.isPending) {
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }
  if (closed.isError) {
    return (
      <p role="alert" className="py-16 text-center text-overdue-ink">
        {closed.error.message}
      </p>
    );
  }

  const items = closed.data.pages.flatMap((page) => page.items);
  return (
    <main>
      <h1 className="mt-6 mb-1 text-[22px] font-semibold tracking-tight">Closed</h1>
      <p className="mb-5 text-[13px] text-ink-soft">
        <span className="max-sm:hidden">Done and won&apos;t-do items, newest first. </span>
        You can reopen an item for a week after it closes.
      </p>
      {items.length === 0 && <p className="py-12 text-center text-muted">Nothing closed yet.</p>}
      {toSections(items).map((section, index) => (
        <ClosedGroupSection
          // Groups can repeat, in the unlikely case the clock moves between pages, so the index keeps keys unique.
          key={`${index}-${section.group}`}
          section={section}
          heapRef={heapRef}
          reopening={reopen.isPending}
          onReopen={(item) => reopen.mutate(item)}
        />
      ))}
      {reopen.error && (
        <p role="alert" className="mt-2 text-sm text-overdue-ink">
          {reopen.error.message}
        </p>
      )}
      {closed.hasNextPage && (
        <div className="mt-4 flex justify-center">
          <button
            type="button"
            onClick={() => void closed.fetchNextPage()}
            disabled={closed.isFetchingNextPage}
            className="rounded-[7px] px-3 py-[5px] text-[13px] text-muted hover:bg-sunken hover:text-ink pointer-coarse:min-h-11"
          >
            Load more
          </button>
        </div>
      )}
    </main>
  );
}

function ClosedGroupSection({
  section,
  heapRef,
  reopening,
  onReopen,
}: {
  section: ClosedSection;
  heapRef: HeapRef;
  reopening: boolean;
  onReopen: (item: Item) => void;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="mb-5">
      <h2
        id={headingId}
        className="mb-1.5 px-1 text-[11px] font-semibold tracking-[0.06em] text-muted uppercase max-sm:text-xs"
      >
        {CLOSED_GROUP_LABELS[section.group]}
      </h2>
      <ol className="divide-line rounded-xl border border-line bg-surface p-1 max-sm:divide-y max-sm:rounded-[14px]">
        {section.items.map((item) => (
          <ClosedRow key={item.id} item={item} heapRef={heapRef} reopening={reopening} onReopen={onReopen} />
        ))}
      </ol>
    </section>
  );
}

/** One closed item: its state mark (wide screens only), the title, "Done · today by you", and Reopen while allowed. */
function ClosedRow({
  item,
  heapRef,
  reopening,
  onReopen,
}: {
  item: Item;
  heapRef: HeapRef;
  reopening: boolean;
  onReopen: (item: Item) => void;
}) {
  const by = useUserName(item.closed_by);
  const wontDo = item.state === "wont_do";
  const became = item.state === "became";
  return (
    <li className="flex items-center gap-3 px-2 py-[9px]">
      <ClosedMark wontDo={wontDo || became} />
      <div className="min-w-0 flex-1">
        <Link
          to="/teams/$team/heaps/$heap/items/$item"
          params={{ ...heapRef, item: item.key }}
          state={(prev) => ({ ...prev, closesBack: true })}
          className={`font-medium break-words hover:underline ${wontDo ? "text-ink-soft" : "text-ink"}`}
        >
          {item.title}
        </Link>
        <p className="text-xs text-muted max-sm:text-[13px]">
          {became ? becameText(item) : STATE_LABELS[item.state]} · {item.closed_label}
          {by && ` by ${by}`}
        </p>
      </div>
      {item.allowed_actions.includes("reopen") && (
        <button
          type="button"
          disabled={reopening}
          onClick={() => onReopen(item)}
          className="shrink-0 rounded-[7px] border border-line px-3 py-[5px] text-[13px] text-ink hover:bg-sunken disabled:opacity-60 max-sm:min-h-11 max-sm:rounded-xl"
        >
          Reopen
        </button>
      )}
    </li>
  );
}

/**
 * What a note that became says: "Became 3 tasks", "Became heap “<name>”" when a heap was made, or "Became a step of
 * “<task>”" when only a step was made.
 */
function becameText(item: Item): string {
  const made = item.became ?? [];
  const heap = item.became_heaps?.[0];
  if (heap) return `Became heap “${heap.name}”`;
  const step = item.became_steps?.[0];
  if (made.length === 0 && step) return `Became a step of “${step.task.title}”`;
  return `Became ${becameWords(made)}`;
}

/**
 * The state at a glance on a wide screen: a filled circle with a tick for Done, an outlined one with a dash for Won't
 * do. Hidden from screen readers, since the meta line says the state.
 */
function ClosedMark({ wontDo }: { wontDo: boolean }) {
  return (
    <svg viewBox="0 0 22 22" aria-hidden="true" className="size-[22px] shrink-0 max-sm:hidden">
      {wontDo ? (
        <>
          <circle cx="11" cy="11" r="10" fill="none" stroke="var(--muted)" strokeWidth="1.5" />
          <path d="M7.5 11h7" stroke="var(--muted)" strokeWidth="1.75" strokeLinecap="round" />
        </>
      ) : (
        <>
          <circle cx="11" cy="11" r="11" fill="var(--done)" />
          <path
            d="m6.75 11.25 2.75 2.75 5.75-5.75"
            fill="none"
            stroke="var(--surface)"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      )}
    </svg>
  );
}
