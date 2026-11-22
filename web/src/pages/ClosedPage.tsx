import { Link } from "@tanstack/react-router";
import { useId } from "react";

import { isNotFound, type HeapRef } from "../api/client";
import type { ClosedGroup, Item } from "../api/schema";
import { useReopenAcrossHeaps, useReopenItem } from "../api/queries/items";
import { useClosedAcrossHeaps, useClosedItems } from "../api/queries/lists";
import { useMe } from "../api/queries/me";
import { acrossSubtitle } from "../components/acrossHeaps";
import { becameWords, CLOSED_GROUP_LABELS, STATE_LABELS } from "../components/labels";
import { useUserName } from "../components/userName";
import { HeapNotFound, TeamNotFound } from "../components/NotFound";
import { teamName } from "../components/teamName";
import { RequestId } from "../components/RequestId";
import { Saving } from "../components/Saving";

/** A closed item on screen: the item, the heap it is in, and what names that heap on the list across heaps. */
interface ClosedEntry {
  item: Item;
  heapRef: HeapRef;
  heapLabel?: string;
}

/** A run of entries next to each other with the same `closed_group`, as the server groups them. */
interface ClosedSection {
  group: ClosedGroup;
  entries: ClosedEntry[];
}

/**
 * Splits `entries`, in server order, into runs that share a `closed_group`. Every page loaded so far is one list, so a
 * group that goes on into the next page stays one section.
 */
function toSections(entries: ClosedEntry[]): ClosedSection[] {
  const sections: ClosedSection[] = [];
  for (const entry of entries) {
    // Closed items always have a group; the fallback only satisfies the type.
    const group = entry.item.closed_group ?? "earlier";
    const last = sections.at(-1);
    if (last && last.group === group) {
      last.entries.push(entry);
    } else {
      sections.push({ group, entries: [entry] });
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
    if (isNotFound(closed.error)) return <HeapNotFound />;
    return <LoadFailed error={closed.error} />;
  }

  return (
    <ClosedList
      intro="Done and won't-do items, newest first. "
      entries={closed.data.pages.flatMap((page) => page.items.map((item) => ({ item, heapRef })))}
      reopening={reopen.isPending ? reopen.variables.id : null}
      onReopen={({ item }) => reopen.mutate(item)}
      reopenError={reopen.error}
      more={closed}
    />
  );
}

/**
 * The closed items of all the person's heaps, or of one team's, in one list (ux.md, All items): the heap's Closed
 * page, each row naming its heap.
 */
export function AllClosedPage({ team }: { team: string | undefined }) {
  const { data: me } = useMe();
  const closed = useClosedAcrossHeaps(team);
  const reopen = useReopenAcrossHeaps();

  if (closed.isPending) {
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }
  if (closed.isError) {
    if (isNotFound(closed.error)) return <TeamNotFound />;
    return <LoadFailed error={closed.error} />;
  }

  // The team is named only where the rows are of several: with one chosen, or for a person in one, the heap is enough.
  const several = team === undefined && (me?.teams.length ?? 0) > 1;
  return (
    <ClosedList
      intro={acrossSubtitle(me, team, "newest first. ")}
      entries={closed.data.pages.flatMap((page) =>
        page.items.map(({ item, heap }) => {
          const ofTeam = me?.teams.find((t) => t.slug === heap.team_slug);
          return {
            item,
            heapRef: { team: heap.team_slug, heap: heap.slug },
            heapLabel: several && me && ofTeam ? `${heap.name} · ${teamName(ofTeam, me)}` : heap.name,
          };
        }),
      )}
      reopening={reopen.isPending ? reopen.variables.item.id : null}
      onReopen={({ item, heapRef }) => reopen.mutate({ heap: heapRef, item })}
      reopenError={reopen.error}
      more={closed}
    />
  );
}

function LoadFailed({ error }: { error: Error }) {
  return (
    <>
      <p role="alert" className="py-16 text-center text-overdue-ink">
        {error.message}
      </p>
      <RequestId error={error} className="justify-center" />
    </>
  );
}

/** The Closed page's heading, groups and Load more, for one heap's items or several heaps'. */
function ClosedList({
  intro,
  entries,
  reopening,
  onReopen,
  reopenError,
  more,
}: {
  /** What the list holds, which a phone leaves out. */
  intro: string;
  entries: ClosedEntry[];
  /** The id of the item being reopened, which every Reopen waits for. */
  reopening: string | null;
  onReopen: (entry: ClosedEntry) => void;
  reopenError: Error | null;
  more: { hasNextPage: boolean; isFetchingNextPage: boolean; fetchNextPage: () => unknown };
}) {
  return (
    <main>
      <h1 className="mt-6 mb-1 text-[22px] font-semibold tracking-tight">Closed</h1>
      <p className="mb-5 text-[13px] text-ink-soft">
        <span className="max-sm:hidden">{intro}</span>
        You can reopen an item for a week after it closes.
      </p>
      {entries.length === 0 && <p className="py-12 text-center text-muted">Nothing closed yet.</p>}
      {toSections(entries).map((section, index) => (
        <ClosedGroupSection
          // Groups can repeat, in the unlikely case the clock moves between pages, so the index keeps keys unique.
          key={`${index}-${section.group}`}
          section={section}
          reopening={reopening}
          onReopen={onReopen}
        />
      ))}
      {reopenError && (
        <>
          <p role="alert" className="mt-2 text-sm text-overdue-ink">
            {reopenError.message}
          </p>
          <RequestId error={reopenError} />
        </>
      )}
      {more.hasNextPage && (
        <div className="mt-4 flex justify-center">
          <button
            type="button"
            onClick={() => void more.fetchNextPage()}
            disabled={more.isFetchingNextPage}
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
  reopening,
  onReopen,
}: {
  section: ClosedSection;
  reopening: string | null;
  onReopen: (entry: ClosedEntry) => void;
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
        {section.entries.map((entry) => (
          <ClosedRow key={entry.item.id} entry={entry} reopening={reopening} onReopen={onReopen} />
        ))}
      </ol>
    </section>
  );
}

/**
 * One closed item: its state mark (wide screens only), the title, "Done · today by you", its heap on the list across
 * heaps, and Reopen while allowed.
 */
function ClosedRow({
  entry,
  reopening,
  onReopen,
}: {
  entry: ClosedEntry;
  reopening: string | null;
  onReopen: (entry: ClosedEntry) => void;
}) {
  const { item, heapRef, heapLabel } = entry;
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
        <p className="text-xs break-words text-muted max-sm:text-[13px]">
          {became ? becameText(item) : STATE_LABELS[item.state]} · {item.closed_label}
          {by && ` by ${by}`}
          {heapLabel && ` · ${heapLabel}`}
        </p>
      </div>
      {item.allowed_actions.includes("reopen") && (
        <button
          type="button"
          disabled={reopening !== null}
          aria-busy={reopening === item.id || undefined}
          onClick={() => onReopen(entry)}
          className="inline-flex shrink-0 items-center gap-2 rounded-[7px] border border-line px-3 py-[5px] text-[13px] text-ink hover:bg-sunken disabled:not-aria-busy:opacity-60 max-sm:min-h-11 max-sm:rounded-xl"
        >
          <Saving pending={reopening === item.id} />
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
