// A history from the audit log, as the server describes it. Entries and their changes come ready to show; this only
// lays them out.
import { ChevronRight } from "lucide-react";
import { useState } from "react";

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
  const log = useAuditLog(scope, entityId);

  if (log.isPending) {
    return <p className="py-4 text-sm text-muted">Loading history…</p>;
  }
  if (log.isError) {
    return (
      <p role="alert" className="py-4 text-sm text-overdue-ink">
        {log.error.message}
      </p>
    );
  }

  const entries = log.data.pages.flatMap((page) => page.entries);
  return (
    <div>
      {entries.length === 0 ? (
        <p className="py-4 text-sm text-muted">No history you can see.</p>
      ) : (
        <ol className="divide-y divide-line">
          {entries.map((entry) => (
            <AuditEntryRow key={entry.id} entry={entry} showEntity={showEntity} />
          ))}
        </ol>
      )}
      {log.hasNextPage && (
        <button
          type="button"
          onClick={() => void log.fetchNextPage()}
          disabled={log.isFetchingNextPage}
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
      {open && (
        <div className="mt-1 pl-3">
          <AuditHistory scope={scope} entityId={entityId} />
        </div>
      )}
    </details>
  );
}
