// The account's history on the Password card: password changes, reset links made for the user and passwords set with
// one. The server words each line; this only lays them out (design/ux.md, Password).
import type { AccountEvent } from "../api/client";
import { useAccountHistory } from "../api/queries";
import { Disclosure, PagedHistory } from "./AuditHistory";

/** A collapsed "History" section that loads the account's history only once it is opened. */
export function AccountHistoryDisclosure() {
  return (
    <Disclosure label="History" className="mt-3">
      <AccountHistory />
    </Disclosure>
  );
}

function AccountHistory() {
  return (
    <PagedHistory
      history={useAccountHistory()}
      empty="No password changes or resets yet."
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
