import { Ellipsis, Search } from "lucide-react";
import { useId, useRef, useState } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import type { Me, OperatorAccount, SignInsToEnd } from "../api/schema";
import { useMe } from "../api/queries/me";
import {
  useDeactivateAccount,
  useMakeResetLink,
  useOperatorAccountHistory,
  useOperatorAccounts,
  useReactivateAccount,
  useSignInsToEnd,
} from "../api/queries/operator";
import { AccountHistory } from "../components/AccountHistory";
import { compactButtonClass } from "../components/buttons";
import { useCopyText } from "../components/copyLink";
import { InlineConfirm } from "../components/InlineConfirm";
import { PHONE, useMedia } from "../components/media";
import { MenuEntry, MenuPopover } from "../components/Menu";
import { NotFound } from "../components/NotFound";
import { ProblemText } from "../components/ProblemText";
import { RequestId } from "../components/RequestId";
import { Sheet, sheetRowClass } from "../components/Sheet";
import { useReturnFocus } from "../components/useConfirmation";
import { useToast } from "../components/UndoToast";
import { plural } from "../components/words";
import { Avatar } from "./TeamPage";
import { cardClass, moreButtonClass, roleButtonClass } from "./teamStyles";
import { Saving } from "../components/Saving";

/** How many matches the list shows, and how many Load more adds. */
const PAGE_SIZE = 50;

const FILTERS = [
  { id: "all", label: "All accounts" },
  { id: "active", label: "Active" },
  { id: "deactivated", label: "Deactivated" },
  { id: "unconfirmed", label: "Unconfirmed" },
  { id: "operators", label: "Operators" },
] as const;
type Filter = (typeof FILTERS)[number]["id"];

function matchesFilter(account: OperatorAccount, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return !account.deactivated;
    case "deactivated":
      return account.deactivated;
    case "unconfirmed":
      return !account.confirmed;
    case "operators":
      return account.operator;
  }
}

function matchesSearch(account: OperatorAccount, search: string): boolean {
  const text = search.trim().toLowerCase();
  return [account.display_name, account.username, account.email].some((value) => value.toLowerCase().includes(text));
}

/** "2 sign-ins and 1 personal token end." — what turning the account off ends, from the server's counts. */
function endsLine(counts: SignInsToEnd): string {
  const parts = [
    ...(counts.sign_ins > 0 ? [plural(counts.sign_ins, "sign-in")] : []),
    ...(counts.personal_tokens > 0 ? [plural(counts.personal_tokens, "personal token")] : []),
  ];
  if (parts.length === 0) return "No sign-ins are open.";
  return `${parts.join(" and ")} ${counts.sign_ins + counts.personal_tokens === 1 ? "ends" : "end"}.`;
}

const confirmQuestion = (account: OperatorAccount, counts: SignInsToEnd) =>
  `Turn off ${account.display_name}'s account? ${endsLine(counts)} Their items and teams stay.`;

/**
 * Accounts (ux.md, Accounts): every account on the server, for an operator. Anyone else gets the not-found page,
 * as the server's `404` for the page's queries would give them.
 */
export function AccountsPage() {
  const { data: me } = useMe();
  if (!me) return null;
  if (!me.operator) {
    return (
      <NotFound
        heading="Page not found"
        line="There's nothing at this address. Check the link, or start again from Waiting for me."
      />
    );
  }
  return <Accounts me={me} />;
}

function Accounts({ me }: { me: Me }) {
  const accounts = useOperatorAccounts();
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [shown, setShown] = useState(PAGE_SIZE);
  const phone = useMedia(PHONE);
  const filterId = useId();

  const matches = (accounts.data ?? []).filter(
    (account) => matchesFilter(account, filter) && matchesSearch(account, search),
  );
  return (
    <main className="flex flex-col gap-3.5 text-sm max-sm:text-[15px]">
      <div className="mt-6 flex flex-col gap-1">
        <h1 className="text-[22px] font-semibold tracking-[-0.015em]">Accounts</h1>
        <p className="text-[13px] text-ink-soft max-sm:text-sm">Every account on this server.</p>
      </div>
      <div className="flex gap-2 max-sm:flex-col">
        <label className="flex h-10 min-w-0 flex-1 items-center gap-2.5 rounded-[10px] border border-line-strong bg-surface px-3 focus-within:border-accent focus-within:ring-[3px] focus-within:ring-accent-soft max-sm:h-12 max-sm:rounded-xl">
          <Search aria-hidden="true" className="size-4 shrink-0 text-muted" />
          <span className="sr-only">Search accounts</span>
          <input
            type="search"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value);
              setShown(PAGE_SIZE);
            }}
            placeholder="Name, username or email"
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-muted max-sm:text-base"
          />
        </label>
        <label
          htmlFor={filterId}
          className="inline-flex h-10 shrink-0 items-center rounded-[10px] border border-line-strong bg-surface pl-3 max-sm:h-12 max-sm:rounded-xl"
        >
          <span className="sr-only">Show</span>
          <select
            id={filterId}
            value={filter}
            onChange={(event) => {
              setFilter(event.target.value as Filter);
              setShown(PAGE_SIZE);
            }}
            className="h-full cursor-pointer rounded-lg bg-transparent pr-2 pl-0 font-medium text-ink outline-offset-2 max-sm:text-base"
          >
            {FILTERS.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
      </div>
      {accounts.isPending && <p className="py-16 text-center text-muted">Loading…</p>}
      {accounts.isError && (
        <>
          <p role="alert" className="py-16 text-center text-overdue-ink">
            {accounts.error.message}
          </p>
          <RequestId error={accounts.error} className="justify-center" />
        </>
      )}
      {accounts.data &&
        (matches.length === 0 ? (
          <p role="status" className="py-6 text-center text-muted">
            No accounts match.
          </p>
        ) : (
          <ul className={cardClass}>
            {matches.slice(0, shown).map((account) => (
              <AccountRow key={account.id} account={account} you={account.id === me.user.id} phone={phone} />
            ))}
          </ul>
        ))}
      {matches.length > shown && (
        <button
          type="button"
          onClick={() => setShown((count) => count + PAGE_SIZE)}
          className={`${compactButtonClass.bordered} self-center`}
        >
          Load more
        </button>
      )}
    </main>
  );
}

/** What opens under a row, or in its sheet on a phone. */
type Panel = { kind: "link"; url: string } | { kind: "history" } | { kind: "confirm"; counts: SignInsToEnd };

type RowAction = "link" | "deactivate" | "reactivate" | "history";

const labelClass = "shrink-0 rounded-md bg-sunken px-2 py-0.5 text-xs text-ink-soft";

/**
 * An account, as the Team page's member row is: avatar, name, @username and email, small labels, the last sign-in on
 * the right, and a ⋯ with the actions the account allows. A deactivated account is dimmed and stays in the list. A
 * provider account and your own row have History only. On a phone the ⋯ opens a sheet that holds the actions and what
 * they open.
 */
function AccountRow({ account, you, phone }: { account: OperatorAccount; you: boolean; phone: boolean }) {
  const toast = useToast();
  const makeLink = useMakeResetLink();
  const countSignIns = useSignInsToEnd();
  const deactivate = useDeactivateAccount();
  const reactivate = useReactivateAccount();
  const [panel, setPanel] = useState<Panel | null>(null);
  const [sheet, setSheet] = useState(false);
  // After Done or Cancel, focus goes back to the ⋯ the panel came from. A phone's sheet gives it back itself.
  const { ref: moreButton, returnFocus } = useReturnFocus<HTMLButtonElement>(panel !== null);
  const name = `${account.display_name}${you ? " (you)" : ""}`;
  // The server says which actions the account allows; the page only labels them.
  const allowed = new Set(account.allowed_actions);
  const actions: { id: RowAction; label: string }[] = [
    ...(allowed.has("make_reset_link") ? [{ id: "link", label: "Make reset link" } as const] : []),
    ...(allowed.has("deactivate") ? [{ id: "deactivate", label: "Deactivate…" } as const] : []),
    ...(allowed.has("reactivate") ? [{ id: "reactivate", label: "Reactivate" } as const] : []),
    ...(allowed.has("history")
      ? [{ id: "history", label: panel?.kind === "history" ? "Hide history" : "History" } as const]
      : []),
  ];

  const resetRequests = () => {
    makeLink.reset();
    countSignIns.reset();
    deactivate.reset();
  };
  const closePanel = () => {
    if (!phone) returnFocus();
    setPanel(null);
    setSheet(false);
    resetRequests();
  };
  const act = (id: RowAction) => {
    resetRequests();
    if (id === "link") {
      makeLink.mutate(account, { onSuccess: (made) => setPanel({ kind: "link", url: made.url }) });
    } else if (id === "deactivate") {
      countSignIns.mutate(account, { onSuccess: (counts) => setPanel({ kind: "confirm", counts }) });
    } else if (id === "reactivate") {
      setSheet(false);
      reactivate.mutate(account, {
        onSuccess: () =>
          toast({
            message: `${account.display_name}'s account is back on.`,
            // Nothing to confirm: the account has no sign-ins left to end.
            undo: () => deactivate.mutate(account),
          }),
      });
    } else {
      setPanel((current) => (current?.kind === "history" ? null : { kind: "history" }));
    }
  };
  const problem = makeLink.error ?? countSignIns.error ?? reactivate.error;
  const confirm = (counts: SignInsToEnd) => confirmQuestion(account, counts);

  return (
    <li className="flex flex-col">
      <div className="flex items-center gap-3 px-2 py-[9px] max-sm:gap-2.5 max-sm:py-2 max-sm:pr-1 max-sm:pl-3">
        <div
          className={`flex min-w-0 flex-1 items-center gap-3 max-sm:gap-2.5 ${account.deactivated ? "opacity-60" : ""}`}
        >
          <Avatar name={account.display_name} id={account.id} />
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="font-medium break-words">{name}</span>
            <span className="text-xs break-words text-muted">
              @{account.username} · {account.email}
            </span>
            <span className="mt-1 flex flex-wrap gap-1.5 sm:hidden">
              <Labels account={account} />
            </span>
          </div>
          <span className="flex shrink-0 flex-wrap justify-end gap-1.5 max-sm:hidden">
            <Labels account={account} />
          </span>
          <span className="shrink-0 text-xs text-muted">{lastSignIn(account)}</span>
        </div>
        {phone ? (
          <button
            type="button"
            aria-label={`Actions for ${account.display_name}`}
            onClick={() => setSheet(true)}
            className={moreButtonClass}
          >
            <Ellipsis aria-hidden="true" className="size-4" />
          </button>
        ) : (
          <MenuTrigger>
            <Button ref={moreButton} aria-label={`Actions for ${account.display_name}`} className={moreButtonClass}>
              <Ellipsis aria-hidden="true" className="size-4" />
            </Button>
            <MenuPopover placement="bottom end">
              <Menu
                aria-label={`Actions for ${account.display_name}`}
                className="outline-none"
                onAction={(key) => act(key as RowAction)}
              >
                {actions.map(({ id, label }) => (
                  <MenuEntry key={id} id={id}>
                    {label}
                  </MenuEntry>
                ))}
              </Menu>
            </MenuPopover>
          </MenuTrigger>
        )}
      </div>
      {!phone && (
        <>
          <ProblemText error={problem} className="px-2 pb-2 text-overdue-ink" />
          {panel?.kind === "link" && <LinkPanel url={panel.url} onDone={closePanel} />}
          {panel?.kind === "history" && (
            <div className="px-2 pb-2">
              <OperatorHistory account={account} />
            </div>
          )}
          {panel?.kind === "confirm" && (
            <div className="px-2 pb-2">
              <InlineConfirm
                question={confirm(panel.counts)}
                confirmLabel="Deactivate"
                pending={deactivate.isPending}
                error={deactivate.error}
                onConfirm={() => deactivate.mutate(account, { onSuccess: closePanel })}
                onCancel={closePanel}
              />
            </div>
          )}
        </>
      )}
      {phone && (
        <Sheet
          title={account.display_name}
          isOpen={sheet}
          onClose={closePanel}
          showTitle={false}
          role={panel?.kind === "confirm" ? "alertdialog" : "dialog"}
        >
          <p className="px-3 pt-2.5 pb-1.5 text-[13px] break-words text-muted">
            {account.display_name} · @{account.username}
          </p>
          {panel?.kind === "confirm" ? (
            <div className="flex flex-col gap-2 px-1 pb-1">
              <p className="px-2 pb-1 text-[15px] text-ink">{confirm(panel.counts)}</p>
              <button
                type="button"
                autoFocus
                disabled={deactivate.isPending}
                aria-busy={deactivate.isPending || undefined}
                onClick={() => deactivate.mutate(account, { onSuccess: closePanel })}
                className="flex min-h-13 items-center justify-center gap-2 rounded-xl bg-overdue-ink text-[15px] font-semibold text-surface disabled:not-aria-busy:opacity-40"
              >
                <Saving pending={deactivate.isPending} />
                Deactivate
              </button>
              <button
                type="button"
                onClick={closePanel}
                className="min-h-13 rounded-xl border border-line text-[15px] text-ink"
              >
                Cancel
              </button>
              <ProblemText error={deactivate.error} className="px-2 text-overdue-ink" />
            </div>
          ) : (
            <div className="flex flex-col">
              {panel?.kind === "link" ? (
                <LinkPanel url={panel.url} onDone={closePanel} />
              ) : (
                actions.map(({ id, label }) => (
                  <button key={id} type="button" onClick={() => act(id)} className={sheetRowClass}>
                    {label}
                  </button>
                ))
              )}
              {panel?.kind === "history" && (
                <div className="px-3 pb-2">
                  <OperatorHistory account={account} />
                </div>
              )}
              <ProblemText error={problem} className="px-3 text-overdue-ink" />
            </div>
          )}
        </Sheet>
      )}
    </li>
  );
}

/** The link just made in a read-only field, with Copy and Done. A copy the browser refuses leaves the link selected. */
function LinkPanel({ url, onDone }: { url: string; onDone: () => void }) {
  const field = useRef<HTMLInputElement>(null);
  const { copied, copy } = useCopyText();
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-2 pb-2 max-sm:px-3">
      <input
        ref={field}
        type="text"
        readOnly
        aria-label="Reset link"
        value={url}
        onFocus={(event) => event.target.select()}
        className="min-w-0 flex-1 basis-60 rounded-lg border border-line-strong bg-surface px-2.5 py-1.5 text-sm text-ink max-sm:text-base pointer-coarse:min-h-11"
      />
      <button
        type="button"
        onClick={() =>
          void copy(url).then((ok) => {
            if (!ok) field.current?.select();
          })
        }
        className={roleButtonClass}
      >
        {copied ? "Copied" : "Copy"}
      </button>
      <button type="button" onClick={onDone} className={roleButtonClass}>
        Done
      </button>
    </div>
  );
}

/** The account's history as the operator reads it. Loads once it is mounted, which is when it is opened. */
function OperatorHistory({ account }: { account: OperatorAccount }) {
  return <AccountHistory history={useOperatorAccountHistory(account)} />;
}

function Labels({ account }: { account: OperatorAccount }) {
  return (
    <>
      {account.kind === "provider" && <span className={labelClass}>Provider</span>}
      {!account.confirmed && <span className={labelClass}>Unconfirmed</span>}
      {account.deactivated && <span className={labelClass}>Deactivated</span>}
      {account.operator && (
        <span className="shrink-0 rounded-md bg-accent-soft px-2 py-0.5 text-xs font-medium text-accent-ink">
          Operator
        </span>
      )}
    </>
  );
}

/** The last sign-in as the Sign-ins card words it, capitalised, or "Never". */
function lastSignIn(account: OperatorAccount): string {
  const label = account.last_sign_in_label;
  return label ? label.charAt(0).toUpperCase() + label.slice(1) : "Never";
}
