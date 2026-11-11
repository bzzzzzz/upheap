// The account's history on the Password card: password changes, reset links made for the user and passwords set with
// one. The server words each line; this only lays them out (design/ux.md, Password).
import type { AccountEvent } from "../api/client";
import { useAccountHistory } from "../api/queries";
import { Disclosure } from "./AuditHistory";

/** A collapsed "History" section that loads the account's history only once it is opened. */
export function AccountHistoryDisclosure() {
  return (
    <Disclosure label="History" className="mt-3">
      <AccountHistory />
    </Disclosure>
  );
}

function AccountHistory() {
  const history = useAccountHistory(true);

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

  const events = history.data.pages.flatMap((page) => page.entries);
  return (
    <div>
      {events.length === 0 ? (
        <p className="py-4 text-sm text-muted">No password changes or resets yet.</p>
      ) : (
        <ol className="divide-y divide-line">
          {events.map((event) => (
            <AccountEventRow key={event.id} event={event} />
          ))}
        </ol>
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

function AccountEventRow({ event }: { event: AccountEvent }) {
  return (
    <li className="py-2 text-sm wrap-anywhere">
      <p>
        {event.description}
        <time dateTime={new Date(event.created_at).toISOString()} className="ml-2 text-xs text-muted">
          {event.created_label}
        </time>
      </p>
    </li>
  );
}
