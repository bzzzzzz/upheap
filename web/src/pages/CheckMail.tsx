import { useState, type FormEvent } from "react";

import { ApiProblem } from "../api/client";
import { useSignIn } from "../api/queries/auth";
import { useChangeConfirmationAddress } from "../api/queries/emailConfirmation";
import { compactButtonClass } from "../components/buttons";
import { problemMessage } from "../components/ProblemText";
import { RequestId } from "../components/RequestId";
import { useReturnFocus } from "../components/useConfirmation";
import { inputClass } from "./settingsForm";
import { Heading, type MoveFocus } from "./authStates";
import { Saving } from "../components/Saving";

/** The card replaces the forms, so its heading takes focus as it mounts. */
const MOVE_FOCUS: MoveFocus = { current: true };

/** Why the card shows: an account was just made, or a sign-in was refused until the address is confirmed. */
export type CheckMailCause = { kind: "created"; email: string } | { kind: "refused" };

/**
 * What a server with mail set up shows until the address is confirmed (ux.md, Sign in and create account): after
 * Create account, or after a sign-in with the right password. **Send it again** posts the sign-in with the login and
 * password the form held, which makes the server mail a fresh link; its `403 email_not_confirmed` means sent. A
 * sign-in that goes through (confirmed meanwhile, in another tab) is `onSignedIn`. **Change it** opens a form for the
 * right address, which the server sets for the account the login and password name and mails a link to. The card then
 * keeps the account's username as its login, since the old login may have been the mistyped address.
 */
export function CheckMail({
  cause,
  login: initialLogin,
  password,
  onSignedIn,
}: {
  cause: CheckMailCause;
  login: string;
  password: string;
  onSignedIn: () => void;
}) {
  const signIn = useSignIn();
  const change = useChangeConfirmationAddress();
  const { error } = signIn;
  const sent = error instanceof ApiProblem && error.code === "email_not_confirmed";
  const [login, setLogin] = useState(initialLogin);
  /** The address the last link went to; unknown after a refused sign-in until it is changed. */
  const [sentTo, setSentTo] = useState(cause.kind === "created" ? cause.email : undefined);
  const [open, setOpen] = useState(false);
  const [address, setAddress] = useState("");
  /** The address the link was last changed to, until the form is opened again. */
  const [changedTo, setChangedTo] = useState<string>();
  const { ref: opener, returnFocus } = useReturnFocus<HTMLButtonElement>(open);

  const close = () => {
    returnFocus();
    setOpen(false);
    setAddress("");
    change.reset();
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    change.mutate(
      { login, password, email: address.trim() },
      {
        onSuccess: (changed) => {
          returnFocus();
          setLogin(changed.username);
          setSentTo(changed.email);
          setChangedTo(changed.email);
          signIn.reset();
          setOpen(false);
          setAddress("");
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-3">
      <Heading moveFocus={MOVE_FOCUS}>{cause.kind === "created" ? "Check your mail" : "Confirm your address"}</Heading>
      <p className="text-sm break-words text-ink-soft">
        {sentTo ? `We sent a link to ${sentTo}. It works for a day.` : "We mailed you a new link. It works for a day."}
      </p>
      <button
        type="button"
        disabled={signIn.isPending}
        aria-busy={signIn.isPending || undefined}
        onClick={() => signIn.mutate({ login, password }, { onSuccess: onSignedIn })}
        className="inline-flex h-11 items-center gap-2 self-start rounded-[10px] border border-line-strong bg-surface px-4 text-sm font-semibold text-ink disabled:not-aria-busy:opacity-40 pointer-coarse:min-h-11"
      >
        <Saving pending={signIn.isPending} />
        Send it again
      </button>
      {/* Shown again, and so announced again, for each send: the mutation leaves its error while it is pending. */}
      {sent && (
        <p role="status" className="text-sm text-ink-soft">
          Sent. It can take a minute to arrive.
        </p>
      )}
      {error && !sent && (
        <>
          <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
            {problemMessage(error)}
          </p>
          <RequestId error={error} />
        </>
      )}
      {changedTo !== undefined && !open && (
        <p role="status" className="text-sm break-words text-ink-soft">
          Sent to {changedTo}. It can take a minute to arrive.
        </p>
      )}
      {!open && (
        <p className="text-sm text-ink-soft">
          Wrong address?{" "}
          <button
            ref={opener}
            type="button"
            onClick={() => {
              setChangedTo(undefined);
              setOpen(true);
            }}
            className="font-semibold text-accent-ink underline pointer-coarse:min-h-11"
          >
            Change it
          </button>
        </p>
      )}
      {open && (
        <form
          onSubmit={submit}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              close();
            }
          }}
          noValidate
          className="flex flex-col gap-3"
        >
          <div className="flex flex-col gap-1.5">
            <label htmlFor="right-address-input" className="text-sm font-medium text-ink">
              Right address
            </label>
            <input
              id="right-address-input"
              type="email"
              value={address}
              onChange={(event) => setAddress(event.target.value)}
              autoComplete="email"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              autoFocus
              className={`${inputClass} w-full`}
            />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="submit"
              disabled={change.isPending}
              aria-busy={change.isPending || undefined}
              className={compactButtonClass.primary}
            >
              <Saving pending={change.isPending} />
              Send a link there
            </button>
            <button type="button" onClick={close} className={compactButtonClass.quiet}>
              Cancel
            </button>
          </div>
          {change.error && (
            <>
              <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
                {problemMessage(change.error)}
              </p>
              <RequestId error={change.error} />
            </>
          )}
        </form>
      )}
    </div>
  );
}
