// The Command line card on Account: where the `upheap` CLI is signed in as the user, each with a way to end it
// (design/ux.md, Account). Server build only; the server says so with the `manage_sign_ins` action.
import { Terminal } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";

import { useEndSignIn, useSignIns } from "../api/queries";
import type { ClientSignIn } from "../api/client";
import { BASE } from "../basePath";
import { compactButtonClass } from "./buttons";
import { ProblemText } from "./ProblemText";
import { formatAgo, formatDay, useNow } from "./time";
import { useReturnFocus } from "./useConfirmation";

const code = (text: string) => <code className="font-mono text-[0.92em]">{text}</code>;

/** What to call a sign-in: the machine it named, or else the client. */
const signInName = (signIn: ClientSignIn) => signIn.device_name ?? signIn.client_name;

export function SignInsSection() {
  const signIns = useSignIns(true);
  const end = useEndSignIn();
  const now = useNow();
  const heading = useRef<HTMLHeadingElement>(null);
  /** The sign-in whose Sign out is asking to be confirmed. One at a time. */
  const [asking, setAsking] = useState<string | null>(null);
  /** The name of the sign-in just ended. */
  const [signedOut, setSignedOut] = useState<string | null>(null);

  const ask = (signIn: ClientSignIn) => {
    end.reset();
    setSignedOut(null);
    setAsking(signIn.id);
  };
  const confirm = (signIn: ClientSignIn) =>
    end.mutate(signIn, {
      onSuccess: () => {
        setAsking(null);
        setSignedOut(signInName(signIn));
        // The row, and the button that had focus, are gone.
        heading.current?.focus();
      },
    });

  return (
    <section aria-labelledby="command-line" className="rounded-[14px] border border-line bg-surface p-5">
      <h2 id="command-line" ref={heading} tabIndex={-1} className="text-base font-semibold">
        Command line
      </h2>
      <p className="mt-1 text-sm text-ink-soft">
        Where the {code("upheap")} CLI is signed in as you. Signing one out here ends it on the server, so a lost laptop
        or an old build machine can't keep acting as you.
      </p>
      {signIns.error && <ProblemText error={signIns.error} className="mt-3 text-overdue-ink" />}
      {signIns.data?.length === 0 && (
        <p className="mt-3 text-sm text-ink-soft">
          The CLI isn't signed in anywhere. To use it, install {code("upheap")} and run{" "}
          {code(`upheap auth login --server ${window.location.origin}${BASE}`)}.
        </p>
      )}
      {signIns.data && signIns.data.length > 0 && (
        <ul className="mt-3">
          {signIns.data.map((signIn) => (
            <SignInRow
              key={signIn.id}
              signIn={signIn}
              now={now}
              confirming={asking === signIn.id}
              pending={end.isPending}
              error={end.error}
              onAsk={() => ask(signIn)}
              onCancel={() => {
                end.reset();
                setAsking(null);
              }}
              onConfirm={() => confirm(signIn)}
            />
          ))}
        </ul>
      )}
      <div role="status">
        {signedOut !== null && <p className="mt-3 text-sm text-ink-soft">Signed out {signedOut}.</p>}
      </div>
    </section>
  );
}

function SignInRow({
  signIn,
  now,
  confirming,
  pending,
  error,
  onAsk,
  onCancel,
  onConfirm,
}: {
  signIn: ClientSignIn;
  now: number;
  confirming: boolean;
  pending: boolean;
  error: Error | null;
  onAsk: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { ref: signOutButton, returnFocus } = useReturnFocus<HTMLButtonElement>(confirming);
  const name = signInName(signIn);
  let content: ReactNode;
  if (confirming) {
    content = (
      <div
        role="group"
        aria-label="Confirm sign-out"
        className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-[10px] bg-overdue-soft px-3.5 py-2.5 text-sm"
      >
        <p className="min-w-0 flex-1 basis-60 break-words text-overdue-ink">
          <strong className="font-semibold">Sign out {name}?</strong> The CLI there stops working until someone runs{" "}
          {code("upheap auth login")} again.
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
          <button type="button" autoFocus onClick={onConfirm} disabled={pending} className={compactButtonClass.danger}>
            Sign out
          </button>
        </div>
        <ProblemText error={error} className="text-overdue-ink" />
      </div>
    );
  } else {
    content = (
      <div className="flex flex-wrap items-center gap-3">
        <span aria-hidden className="grid size-9 shrink-0 place-items-center rounded-[9px] bg-sunken text-ink-soft">
          <Terminal className="size-4" />
        </span>
        <div className="min-w-0 flex-1 basis-48">
          <p className="text-sm font-medium break-words text-ink">{name}</p>
          <p className="text-xs text-muted">
            Signed in {formatDay(signIn.signed_in_at, now)} · last used {formatAgo(signIn.last_used_at, now)}
          </p>
        </div>
        <button
          ref={signOutButton}
          type="button"
          aria-label={`Sign out ${name}`}
          onClick={onAsk}
          className={compactButtonClass.borderedDanger}
        >
          Sign out
        </button>
      </div>
    );
  }
  return <li className="border-t border-line py-3">{content}</li>;
}
