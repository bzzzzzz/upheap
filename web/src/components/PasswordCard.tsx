// The Password card on Account: changing the password, which ends the user's other sign-ins (design/ux.md,
// Password). The server says so with the `change_password` action; a user without a password, who signs in through
// the server's OpenID Connect provider, sees the card's frame with a line in place of the button.
import { useRef, useState, type FormEvent, type ReactNode } from "react";

import { ApiProblem } from "../api/client";
import { useChangePassword, useOidcProvider } from "../api/queries";
import { AccountHistoryDisclosure } from "./AccountHistory";
import { compactButtonClass } from "./buttons";
import { problemMessage, ProblemText } from "./ProblemText";
import { useReturnFocus } from "./useConfirmation";

/** The input a problem is about, when it is about one. */
function inputOf(error: Error | null): "current" | "new" | null {
  if (!(error instanceof ApiProblem)) return null;
  if (error.code === "wrong_password" || (error.code === "field_empty" && error.field === "current_password")) {
    return "current";
  }
  if (error.code === "password_too_short" || error.code === "password_too_long") return "new";
  if (error.code === "field_empty" && error.field === "password") return "new";
  return null;
}

/** The words for a problem under the new password's input; an empty one is "a new password", not "your password". */
function newPasswordMessage(error: Error): string {
  return error instanceof ApiProblem && error.code === "field_empty" ? "Enter a new password." : problemMessage(error);
}

function endedMessage(ended: number): string {
  if (ended === 0) return "Password changed. No other sign-ins were open.";
  return `Password changed. ${ended} other sign-in${ended === 1 ? "" : "s"} ended.`;
}

/** The card's frame: its title with `action` beside it, then `children`, then the History disclosure. */
function PasswordFrame({ action, children }: { action?: ReactNode; children?: ReactNode }) {
  return (
    <section aria-labelledby="password" className="rounded-[14px] border border-line bg-surface p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id="password" className="text-base font-semibold">
          Password
        </h2>
        {action}
      </div>
      {children}
      <AccountHistoryDisclosure />
    </section>
  );
}

/** The card of a user who has no password: they sign in through the provider. */
export function NoPasswordCard() {
  const label = useOidcProvider().data?.label ?? "an identity provider";
  return <PasswordFrame action={<p className="text-sm text-ink-soft">You sign in through {label}.</p>} />;
}

/** `inputClass` is the look of the Profile card's input, which the card shares. */
export function PasswordCard({ inputClass }: { inputClass: string }) {
  const change = useChangePassword();
  const [open, setOpen] = useState(false);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  /** How many other sign-ins the last change ended, until the form is opened again. */
  const [ended, setEnded] = useState<number | null>(null);
  const currentInput = useRef<HTMLInputElement>(null);
  const newInput = useRef<HTMLInputElement>(null);
  const { ref: opener, returnFocus } = useReturnFocus<HTMLButtonElement>(open);

  const failed = inputOf(change.error);
  const currentError = failed === "current" && change.error ? problemMessage(change.error) : undefined;
  const newError = failed === "new" && change.error ? newPasswordMessage(change.error) : undefined;

  const close = () => {
    returnFocus();
    setOpen(false);
    setCurrent("");
    setNext("");
    change.reset();
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    // Sent as typed: a password can start or end with a space.
    change.mutate(
      { current_password: current, password: next },
      {
        onSuccess: (changed) => {
          returnFocus();
          setEnded(changed.ended_sign_ins);
          setOpen(false);
          setCurrent("");
          setNext("");
        },
        onError: (error) => {
          const input = inputOf(error);
          // A password manager can fill the right one in again.
          if (error instanceof ApiProblem && error.code === "wrong_password") setCurrent("");
          if (input === "current") currentInput.current?.focus();
          else if (input === "new") newInput.current?.focus();
        },
      },
    );
  };

  return (
    <PasswordFrame
      action={
        !open && (
          <button
            ref={opener}
            type="button"
            onClick={() => {
              setEnded(null);
              setOpen(true);
            }}
            className={compactButtonClass.bordered}
          >
            Change password
          </button>
        )
      }
    >
      {ended !== null && !open && (
        <p role="status" className="mt-3 text-sm text-ink-soft">
          {endedMessage(ended)}
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
          className="mt-4 flex flex-col gap-4"
        >
          <PasswordInput
            label="Current password"
            inputRef={currentInput}
            value={current}
            onChange={(value) => {
              setCurrent(value);
              if (currentError) change.reset();
            }}
            autoComplete="current-password"
            autoFocus
            error={currentError}
            inputClass={inputClass}
          />
          <PasswordInput
            label="New password"
            inputRef={newInput}
            value={next}
            onChange={(value) => {
              setNext(value);
              if (newError) change.reset();
            }}
            autoComplete="new-password"
            hint="At least 10 characters."
            error={newError}
            inputClass={inputClass}
          />
          {failed === null && <ProblemText error={change.error} className="text-overdue-ink" />}
          <div className="flex flex-wrap items-center gap-2">
            <button type="submit" disabled={change.isPending} className={compactButtonClass.primary}>
              Change password
            </button>
            <button type="button" disabled={change.isPending} onClick={close} className={compactButtonClass.quiet}>
              Cancel
            </button>
          </div>
          <p className="-mt-2 text-xs text-muted">This signs you out of your other browsers and the command line.</p>
        </form>
      )}
    </PasswordFrame>
  );
}

function PasswordInput({
  label,
  inputRef,
  value,
  onChange,
  autoComplete,
  autoFocus,
  hint,
  error,
  inputClass,
}: {
  label: string;
  inputRef: React.RefObject<HTMLInputElement | null>;
  value: string;
  onChange: (value: string) => void;
  autoComplete: string;
  autoFocus?: boolean;
  hint?: string;
  error: string | undefined;
  inputClass: string;
}) {
  const id = `${autoComplete}-input`;
  const noteId = `${id}-note`;
  const note = error ?? hint;
  return (
    <div className="flex max-w-sm flex-col gap-1.5">
      <label htmlFor={id} className="text-sm font-medium text-ink">
        {label}
      </label>
      <input
        ref={inputRef}
        id={id}
        type="password"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        autoComplete={autoComplete}
        autoFocus={autoFocus}
        aria-invalid={error ? true : undefined}
        aria-describedby={note ? noteId : undefined}
        className={`${inputClass} aria-invalid:border-overdue-ink`}
      />
      {error ? (
        <p id={noteId} role="alert" className="text-xs font-medium text-overdue-ink">
          {error}
        </p>
      ) : (
        hint && (
          <p id={noteId} className="text-xs text-muted">
            {hint}
          </p>
        )
      )}
    </div>
  );
}
