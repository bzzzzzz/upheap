// The account's History card: password changes, reset links made for the user, passwords set with one, address changes and
// confirmations, an operator's turn-offs and turn-ons, and the user's own heap priority changes. The server words each
// line; this only lays them out (design/ux.md, History). An operator reads the same list for any account, on Accounts.
import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";

import type { AccountEvent, AccountHistory as AccountHistoryPage } from "../api/schema";
import { useAccountHistory } from "../api/queries/audit";
import { Disclosure, PagedHistory } from "./AuditHistory";

/** The History card, with a collapsed disclosure that loads the account's history only once it is opened. */
export function HistoryCard() {
  return (
    <section aria-labelledby="history" className="rounded-[14px] border border-line bg-surface p-5">
      <h2 id="history" className="text-base font-semibold">
        History
      </h2>
      <Disclosure label="Changes to your account" className="mt-3">
        <OwnAccountHistory />
      </Disclosure>
    </section>
  );
}

function OwnAccountHistory() {
  return <AccountHistory history={useAccountHistory()} />;
}

/** The entries of an account's history, read by `history`, a page at a time. */
export function AccountHistory({
  history,
}: {
  history: UseInfiniteQueryResult<InfiniteData<AccountHistoryPage>, Error>;
}) {
  return (
    <PagedHistory
      history={history}
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
