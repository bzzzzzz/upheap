// The Tokens part of the Command line card on Account: the user's personal tokens for a machine with no browser or
// keychain, each with a way to revoke it, and a form that makes one and shows its secret once (ux.md, Command line).
import { KeyRound } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";

import type { PersonalToken, PersonalTokenExpiry } from "../api/schema";
import { useMakePersonalToken, usePersonalTokens, useRevokePersonalToken } from "../api/queries/signIns";
import { BASE } from "../basePath";
import { compactButtonClass } from "./buttons";
import { useCopyText } from "./copyLink";
import { ProblemText } from "./ProblemText";
import { useReturnFocus } from "./useConfirmation";
import { Saving } from "./Saving";

const code = (text: string) => <code className="font-mono text-[0.92em]">{text}</code>;

const fieldClass =
  "rounded-lg border border-line bg-surface px-2.5 py-2 text-sm text-ink max-sm:text-base pointer-coarse:min-h-11";

const expiries: readonly { value: PersonalTokenExpiry; label: string }[] = [
  { value: "30_days", label: "30 days" },
  { value: "90_days", label: "90 days" },
  { value: "1_year", label: "1 year" },
  { value: "never", label: "Never" },
];

/**
 * `asking` is the id of the sign-in or token whose confirm is open, which the card shares so that only one is. The card
 * says what happened with `onRevoked`, and moves focus to its heading.
 */
export function PersonalTokensSection({
  asking,
  onAsk,
  onCancel,
  onRevoked,
}: {
  asking: string | null;
  onAsk: (id: string) => void;
  onCancel: () => void;
  onRevoked: (name: string) => void;
}) {
  const tokens = usePersonalTokens();
  const revoke = useRevokePersonalToken();

  return (
    <div className="mt-5 border-t border-line pt-4">
      <h3 className="text-sm font-semibold text-ink">Tokens</h3>
      <p className="mt-1 text-sm text-ink-soft">
        For a machine with no browser or keychain, such as CI or a container: give it a token in {code("UPHEAP_TOKEN")}.
        A token can do anything you can.
      </p>
      {tokens.error && <ProblemText error={tokens.error} className="mt-3 text-overdue-ink" />}
      {tokens.data?.length === 0 && <p className="mt-3 text-sm text-ink-soft">No tokens yet.</p>}
      {tokens.data && tokens.data.length > 0 && (
        <ul className="mt-3">
          {tokens.data.map((token) => (
            <TokenRow
              key={token.id}
              token={token}
              confirming={asking === token.id}
              pending={revoke.isPending}
              error={revoke.error}
              onAsk={() => {
                revoke.reset();
                onAsk(token.id);
              }}
              onCancel={() => {
                revoke.reset();
                onCancel();
              }}
              onConfirm={() => revoke.mutate(token, { onSuccess: () => onRevoked(token.name) })}
            />
          ))}
        </ul>
      )}
      <MakeToken />
    </div>
  );
}

function TokenRow({
  token,
  confirming,
  pending,
  error,
  onAsk,
  onCancel,
  onConfirm,
}: {
  token: PersonalToken;
  confirming: boolean;
  pending: boolean;
  error: Error | null;
  onAsk: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { ref: revokeButton, returnFocus } = useReturnFocus<HTMLButtonElement>(confirming);
  const name = token.name;
  let content: ReactNode;
  if (confirming) {
    content = (
      <div
        role="group"
        aria-label="Confirm revoke"
        className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-[10px] bg-overdue-soft px-3.5 py-2.5 text-sm"
      >
        <p className="min-w-0 flex-1 basis-60 break-words text-overdue-ink">
          <strong className="font-semibold">Revoke {name}?</strong> Anything using it stops working at once.
        </p>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              returnFocus();
              onCancel();
            }}
            disabled={pending}
            className={compactButtonClass.quiet}
          >
            Cancel
          </button>
          <button
            type="button"
            autoFocus
            onClick={onConfirm}
            disabled={pending}
            aria-busy={pending || undefined}
            className={compactButtonClass.danger}
          >
            <Saving pending={pending} />
            Revoke
          </button>
        </div>
        <ProblemText error={error} className="text-overdue-ink" />
      </div>
    );
  } else {
    content = (
      <div className="flex flex-wrap items-center gap-3">
        <span aria-hidden className="grid size-9 shrink-0 place-items-center rounded-[9px] bg-sunken text-ink-soft">
          <KeyRound className="size-4" />
        </span>
        <div className="min-w-0 flex-1 basis-48">
          <p className="text-sm font-medium break-words text-ink">{name}</p>
          <p className="text-xs text-muted">
            Made {token.created_label} · last used {token.last_used_label ?? "never"} ·{" "}
            {token.expires_label ? `expires ${token.expires_label}` : "never expires"}
          </p>
        </div>
        <button
          ref={revokeButton}
          type="button"
          aria-label={`Revoke ${name}`}
          onClick={onAsk}
          disabled={pending}
          className={compactButtonClass.borderedDanger}
        >
          Revoke
        </button>
      </div>
    );
  }
  return <li className="border-t border-line py-3">{content}</li>;
}

/** The token just made. Its secret lives only here, in state: leaving the page drops it. */
type Made = { name: string; secret: string };

/** The Make a token button, which the form replaces, and then the panel that shows the secret once. */
function MakeToken() {
  const make = useMakePersonalToken();
  const [open, setOpen] = useState(false);
  const [made, setMade] = useState<Made | null>(null);
  const { ref: makeButton, returnFocus } = useReturnFocus<HTMLButtonElement>(open || made !== null);

  const close = () => {
    make.reset();
    setOpen(false);
    setMade(null);
    returnFocus();
  };

  if (made) return <MadePanel made={made} onDone={close} />;
  if (open) {
    return (
      <MakeForm
        pending={make.isPending}
        error={make.error}
        onCancel={close}
        onMake={(name, expires_in) =>
          make.mutate(
            { name, expires_in },
            {
              onSuccess: (result) => {
                setOpen(false);
                setMade({ name: result.token.name, secret: result.secret });
                // The answer, with the secret, is dropped from the mutation as the secret moves to state.
                make.reset();
              },
            },
          )
        }
      />
    );
  }
  return (
    <button
      ref={makeButton}
      type="button"
      onClick={() => {
        make.reset();
        setOpen(true);
      }}
      className={`${compactButtonClass.quiet} mt-2 -ml-3`}
    >
      Make a token
    </button>
  );
}

function MakeForm({
  pending,
  error,
  onCancel,
  onMake,
}: {
  pending: boolean;
  error: Error | null;
  onCancel: () => void;
  onMake: (name: string, expires: PersonalTokenExpiry) => void;
}) {
  const [name, setName] = useState("");
  const [expires, setExpires] = useState<PersonalTokenExpiry>("90_days");
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (name.trim() === "") return;
    onMake(name, expires);
  };
  return (
    <form
      aria-label="Make a token"
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onCancel();
        }
      }}
      className="mt-3 flex flex-wrap items-end gap-x-3 gap-y-3"
    >
      <label className="flex min-w-0 flex-1 basis-48 flex-col gap-1.5">
        <span className="text-xs font-medium text-ink-soft">Name</span>
        <input
          autoFocus
          required
          maxLength={64}
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="ci, laptop container…"
          className={fieldClass}
        />
      </label>
      <label className="flex min-w-0 basis-36 flex-col gap-1.5">
        <span className="text-xs font-medium text-ink-soft">Expires</span>
        <select
          value={expires}
          onChange={(event) => setExpires(event.target.value as PersonalTokenExpiry)}
          className={fieldClass}
        >
          {expiries.map((expiry) => (
            <option key={expiry.value} value={expiry.value}>
              {expiry.label}
            </option>
          ))}
        </select>
      </label>
      <div className="flex items-center gap-2">
        <button
          type="submit"
          disabled={pending || name.trim() === ""}
          aria-busy={pending || undefined}
          className={compactButtonClass.primary}
        >
          <Saving pending={pending} />
          Make token
        </button>
        <button type="button" onClick={onCancel} className={compactButtonClass.quiet}>
          Cancel
        </button>
      </div>
      <ProblemText error={error} className="text-overdue-ink" />
    </form>
  );
}

/** The secret, shown once, with how to use it. */
function MadePanel({ made, onDone }: { made: Made; onDone: () => void }) {
  const { copied, copy } = useCopyText();
  const field = useRef<HTMLInputElement>(null);
  // The clipboard wasn't there: the secret is selected to copy by hand.
  const [manual, setManual] = useState(false);
  const copyButton = useRef<HTMLButtonElement>(null);
  useEffect(() => copyButton.current?.focus(), []);

  const copySecret = async () => {
    const ok = await copy(made.secret);
    setManual(!ok);
    if (!ok) {
      field.current?.focus();
      field.current?.select();
    }
  };

  return (
    <div role="group" aria-label="New token" className="mt-3 rounded-[10px] bg-sunken px-3.5 py-3 text-sm">
      <p className="break-words text-ink">
        <strong className="font-semibold">{made.name}</strong> is made. Copy it now: it won't be shown again.
      </p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <input
          ref={field}
          readOnly
          aria-label="Token"
          value={made.secret}
          onFocus={(event) => event.currentTarget.select()}
          className={`${fieldClass} min-w-0 flex-1 basis-60 font-mono`}
        />
        <button
          ref={copyButton}
          type="button"
          onClick={() => void copySecret()}
          className={compactButtonClass.bordered}
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <p aria-live="polite" className="mt-1 text-xs text-ink-soft empty:hidden">
        {manual ? "Select it and copy it by hand." : null}
      </p>
      <p className="mt-2 text-ink-soft">To use it:</p>
      <p className="mt-1 font-mono text-[0.92em] break-all text-ink">
        {`UPHEAP_SERVER=${window.location.origin}${BASE} UPHEAP_TOKEN=<token> upheap item list`}
      </p>
      <div className="mt-3">
        <button type="button" onClick={onDone} className={compactButtonClass.primary}>
          Done
        </button>
      </div>
    </div>
  );
}
