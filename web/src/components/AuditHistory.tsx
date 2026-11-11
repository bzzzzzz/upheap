// A history from the audit log, as the server describes it. Entries and their changes come ready to show; this only
// lays them out.
import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import { ChevronRight } from "lucide-react";
import { useState, type ReactNode } from "react";

import type { AuditEntry } from "../api/client";
import { useAuditLog, useMe, type AuditScope } from "../api/queries";

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
      <p role="alert" className="py-4 text-sm text-overdue-ink">
        {history.error.message}
      </p>
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

function AuditEntryRow({ entry, showEntity }: { entry: AuditEntry; showEntity: boolean }) {
  const { data: me } = useMe();
  // The viewer's own changes read as theirs, as the phrase reads after any name: "You added item".
  const actor = entry.actor.id === me?.user.id ? "You" : entry.actor.display_name;
  return (
    <li className="py-2 text-sm wrap-anywhere">
      <p>
        <span className="font-medium">{actor}</span> {entry.phrase}
        {showEntity && entry.names_entity && <span className="font-medium"> {entry.entity.label}</span>}
        <time dateTime={new Date(entry.created_at).toISOString()} className="ml-2 text-xs text-muted">
          {entry.created_label}
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

function ChangeValues({ before, after }: { before?: string | null; after?: string | null }) {
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
  children,
}: {
  label: string;
  /** Where it sits: its margins, or how much of a row it takes. */
  className: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details onToggle={(event) => setOpen(event.currentTarget.open)} className={`group ${className}`}>
      <summary className="inline-flex min-h-9 cursor-pointer list-none items-center gap-1 text-[13px] font-medium text-ink-soft select-none hover:text-ink pointer-coarse:min-h-11 [&::-webkit-details-marker]:hidden">
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
}: {
  scope: AuditScope;
  entityId?: string;
  label?: string;
  /** Where it sits: its margins, or how much of a row it takes. */
  className?: string;
}) {
  return (
    <Disclosure label={label} className={className}>
      <AuditHistory scope={scope} entityId={entityId} />
    </Disclosure>
  );
}
