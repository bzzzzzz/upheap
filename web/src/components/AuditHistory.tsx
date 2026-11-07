// A history from the audit log, as the server describes it. Entries and their changes come ready to show; this only
// lays them out.
import { useState } from "react";

import type { AuditEntry } from "../api/client";
import { useAuditLog, type AuditScope } from "../api/queries";

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
    return <p className="py-4 text-sm text-stone-500">Loading history…</p>;
  }
  if (log.isError) {
    return (
      <p role="alert" className="py-4 text-sm text-red-700 dark:text-red-400">
        {log.error.message}
      </p>
    );
  }

  const entries = log.data.pages.flatMap((page) => page.entries);
  return (
    <div>
      {entries.length === 0 ? (
        <p className="py-4 text-sm text-stone-500">No history you can see.</p>
      ) : (
        <ol className="divide-y divide-stone-200 dark:divide-stone-800">
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
          className="mt-2 text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          Load more
        </button>
      )}
    </div>
  );
}

function AuditEntryRow({ entry, showEntity }: { entry: AuditEntry; showEntity: boolean }) {
  return (
    <li className="py-2 text-sm">
      <p>
        <span className="font-medium">{entry.actor.display_name}</span> {entry.phrase}
        {showEntity && entry.names_entity && <span className="font-medium"> {entry.entity.label}</span>}
        <time dateTime={new Date(entry.created_at).toISOString()} className="ml-2 text-xs text-stone-500">
          {entry.created_label}
        </time>
      </p>
      {entry.changes.length > 0 && (
        <ul className="mt-1 space-y-0.5 text-xs text-stone-600 dark:text-stone-400">
          {entry.changes.map((change) => (
            <li key={change.field}>
              <span className="text-stone-500">{change.label}:</span> <ChangeValues {...change} />
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
      <del className="text-stone-500">{before}</del> → {after ?? <span className="italic">none</span>}
    </span>
  );
}

/** A collapsed "History" section that loads the history only once it is opened. */
export function HistoryDisclosure({
  scope,
  entityId,
  label = "History",
}: {
  scope: AuditScope;
  entityId?: string;
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details onToggle={(event) => setOpen(event.currentTarget.open)} className="mt-1">
      <summary className="cursor-pointer text-xs text-stone-500 select-none hover:text-stone-900 dark:hover:text-stone-100">
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
