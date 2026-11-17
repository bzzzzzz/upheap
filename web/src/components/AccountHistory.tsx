// The account's History card: password changes, reset links made for the user, passwords set with one and the user's
// own heap priority changes. The server words each line; this only lays them out (design/ux.md, History).
import type { AccountEvent } from "../api/schema";
import { useAccountHistory } from "../api/queries";
import { Disclosure, PagedHistory } from "./AuditHistory";

/** The History card, with a collapsed disclosure that loads the account's history only once it is opened. */
export function HistoryCard() {
  return (
    <section aria-labelledby="history" className="rounded-[14px] border border-line bg-surface p-5">
      <h2 id="history" className="text-base font-semibold">
        History
      </h2>
      <Disclosure label="Changes to your account" className="mt-3">
        <AccountHistory />
      </Disclosure>
    </section>
  );
}

function AccountHistory() {
  return (
    <PagedHistory
      history={useAccountHistory()}
      empty="No account changes yet."
      row={(event) => <AccountEventRow key={event.id} event={event} />}
    />
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
