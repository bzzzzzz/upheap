// A history from the audit log, as the server describes it. Entries and their changes come ready to show; this only
// lays them out.
import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { Bot, ChevronRight } from "lucide-react";
import { useState, type ReactNode } from "react";

import type { AuditChange, AuditEntry } from "../api/schema";
import { useAuditLog, type AuditScope } from "../api/queries/audit";
import { useMe } from "../api/queries/me";
import { useOverListLink } from "./overList";
import { TextDiff } from "./TextDiff";
import { RequestId } from "./RequestId";
import { sectionHeadingClass } from "./sectionHeading";

/**
 * The history of `scope`, or of one entity in it, newest first, a page at a time. With `showEntity` each entry
 * names what it is about, for histories that cover more than one thing.
 */
export function AuditHistory({
  scope,
  entityId,
  showEntity = false,
}: {
  scope: AuditScope;
  entityId?: string;
  showEntity?: boolean;
}) {
  return (
    <PagedHistory
      history={useAuditLog(scope, entityId)}
      empty="No history you can see."
      row={(entry) => <AuditEntryRow key={entry.id} entry={entry} showEntity={showEntity} />}
    />
  );
}

/**
 * A history read a page at a time, newest first: its loading, error and empty states, its rows, and Load more. Every
 * history uses it, so they all look and behave alike; `row` lays out one entry as an `<li>`.
 */
export function PagedHistory<Entry>({
  history,
  empty,
  row,
}: {
  history: UseInfiniteQueryResult<InfiniteData<{ entries: Entry[] }>, Error>;
  empty: string;
  row: (entry: Entry) => ReactNode;
}) {
  if (history.isPending) {
    return <p className="py-4 text-sm text-muted">Loading history…</p>;
  }
  if (history.isError) {
    return (
      <>
        <p role="alert" className="py-4 text-sm text-overdue-ink">
          {history.error.message}
        </p>
        <RequestId error={history.error} />
      </>
    );
  }

  const entries = history.data.pages.flatMap((page) => page.entries);
  return (
    <div>
      {entries.length === 0 ? (
        <p className="py-4 text-sm text-muted">{empty}</p>
      ) : (
        <ol className="divide-y divide-line">{entries.map(row)}</ol>
      )}
      {history.hasNextPage && (
        <button
          type="button"
          onClick={() => void history.fetchNextPage()}
          disabled={history.isFetchingNextPage}
          className="mt-2 min-h-9 text-sm text-muted hover:text-ink pointer-coarse:min-h-11"
        >
          Load more
        </button>
      )}
    </div>
  );
}

/** One entry. `place` names where it happened, for a history across heaps. */
export function AuditEntryRow({
  entry,
  showEntity,
  place,
}: {
  entry: AuditEntry;
  showEntity: boolean;
  place?: string;
}) {
  const { data: me } = useMe();
  // The viewer's own changes read as theirs, as the phrase reads after any name: "You added item".
  const own = entry.actor.id === me?.user.id;
  const actor = own ? "You" : entry.actor.display_name;
  return (
    <li className="py-2 text-sm wrap-anywhere">
      <p>
        {entry.run ? (
          <AgentMark
            runId={entry.run.id}
            at={
              entry.run.item && {
                team: entry.run.item.heap.team_slug,
                heap: entry.run.item.heap.slug,
                item: entry.run.item.key,
              }
            }
            who={own ? "Your agent" : `${entry.actor.display_name}'s agent`}
          />
        ) : (
          <span className="font-medium">{actor}</span>
        )}{" "}
        {entry.phrase}
        {showEntity && entry.names_entity && <span className="font-medium"> {entry.entity.label}</span>}
        <time dateTime={new Date(entry.created_at).toISOString()} className="ml-2 text-xs text-muted">
          {entry.created_label}
          {place && ` · ${place}`}
        </time>
      </p>
      {entry.changes.length > 0 && (
        <ul className="mt-1 space-y-0.5 text-xs text-ink-soft">
          {entry.changes.map((change) => (
            <li key={change.field}>
              <span className="text-muted">{change.label}:</span> <ChangeValues {...change} />
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

/**
 * An agent's change reads as the agent's, not the person's whose sign-in it used (ux.md, The item, point 21): a robot and
 * whose agent it is, linking to the run in its item, `at`, when the caller may view that item. An agent's comment is
 * marked the same way (ux.md, The item, Comments).
 */
export function AgentMark({
  runId,
  at,
  who,
}: {
  runId: string;
  at: { team: string; heap: string; item: string } | null;
  who: string;
}) {
  const words = (
    <>
      <Bot aria-hidden="true" className="mr-1 inline size-3.5 align-text-bottom" />
      {who}
    </>
  );
  const open = useParams({ strict: false });
  const overList = useOverListLink();
  if (!at) {
    return <span className="font-medium">{words}</span>;
  }
  const { team, heap, item } = at;
  // The item is already open: the link goes in place of it, so Back and Close still lead where they did.
  const itemOpen = open.team === team && open.heap === heap && open.item === item;
  return (
    <Link
      to="/teams/$team/heaps/$heap/items/$item"
      params={{ team, heap, item }}
      hash={`run-${runId}`}
      search={true}
      replace={itemOpen}
      {...overList({ team, heap }, item)}
      // A fresh value on each click makes each one a new location, which the item's Runs section acts on.
      state={(prev) => ({
        ...prev,
        closesBack: itemOpen ? prev.closesBack : true,
        runLinkedAt: Date.now(),
      })}
      aria-label={`${who}, see the run`}
      className="font-medium text-accent-ink outline-none hover:underline focus-visible:underline focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:inline-flex pointer-coarse:min-h-11 pointer-coarse:items-center"
    >
      {words}
    </Link>
  );
}

/** A change's values: as a word diff when the server sent one, else the old value struck through and the new one. */
function ChangeValues({ before, after, text_diff }: AuditChange) {
  if (text_diff) {
    return <TextDiff diff={text_diff} />;
  }
  if (before == null && after == null) {
    return <span>changed</span>;
  }
  if (before == null) {
    return <span>{after}</span>;
  }
  return (
    <span>
      <del className="text-muted">{before}</del> → {after ?? <span className="italic">none</span>}
    </span>
  );
}

/** A collapsed section, with its chevron, that renders `children` only once it is opened. */
export function Disclosure({
  label,
  className,
  defaultOpen = false,
  section = false,
  children,
}: {
  label: string;
  /** Whether it is a section of the item, whose label reads as the other sections' headings do. */
  section?: boolean;
  /** Where it sits: its margins, or how much of a row it takes. */
  className: string;
  /** Whether it starts opened; a new `key` starts it again. */
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <details
      open={defaultOpen}
      onToggle={(event) => setOpen(event.currentTarget.open)}
      className={`group ${className}`}
    >
      <summary
        className={`inline-flex min-h-9 cursor-pointer list-none items-center gap-1 select-none hover:text-ink pointer-coarse:min-h-11 [&::-webkit-details-marker]:hidden ${section ? sectionHeadingClass : "text-[13px] font-medium text-ink-soft"}`}
      >
        <ChevronRight
          aria-hidden="true"
          className="size-3.5 transition-transform group-open:rotate-90 motion-reduce:transition-none"
        />
        {label}
      </summary>
      {open && <div className="mt-1 pl-3">{children}</div>}
    </details>
  );
}

/** A collapsed "History" section that loads the history only once it is opened. */
export function HistoryDisclosure({
  scope,
  entityId,
  label = "History",
  className = "mt-1",
  showEntity,
  section,
}: {
  scope: AuditScope;
  entityId?: string;
  label?: string;
  /** Each entry names the entity it is about, as in a team's history. */
  showEntity?: boolean;
  /** As `Disclosure`'s. */
  section?: boolean;
  /** Where it sits: its margins, or how much of a row it takes. */
  className?: string;
}) {
  return (
    <Disclosure label={label} className={className} section={section}>
      <AuditHistory scope={scope} entityId={entityId} showEntity={showEntity} />
    </Disclosure>
  );
}
