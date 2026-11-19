// The Email card on Account: changing the address, confirmed by a link mailed to the new one (design/ux.md, Email).
// The server says so with the `change_email` action; without it the address is a read-only row in the Profile card.
import { useRef, useState, type FormEvent } from "react";

import { ApiProblem } from "../api/client";
import { useRequestEmailChange } from "../api/queries/emailChange";
import { compactButtonClass } from "./buttons";
import { PasswordInput } from "./PasswordCard";
import { problemMessage, ProblemText } from "./ProblemText";
import { useReturnFocus } from "./useConfirmation";

/** The input a problem is about, when it is about one. */
function inputOf(error: Error | null): "email" | "password" | null {
  if (!(error instanceof ApiProblem)) return null;
  if (error.code === "email_invalid" || (error.code === "field_empty" && error.field === "email")) return "email";
  if (error.code === "wrong_password" || (error.code === "field_empty" && error.field === "current_password")) {
    return "password";
  }
  return null;
}

/** `inputClass` is the look of the Profile card's input, which the card shares. */
export function EmailCard({ email, inputClass }: { email: string; inputClass: string }) {
  const request = useRequestEmailChange();
  const [open, setOpen] = useState(false);
  const [address, setAddress] = useState("");
  const [password, setPassword] = useState("");
  /** The address the last link was mailed to, until the form is opened again. */
  const [asked, setAsked] = useState<string | null>(null);
  const addressInput = useRef<HTMLInputElement>(null);
  const passwordInput = useRef<HTMLInputElement>(null);
  const { ref: opener, returnFocus } = useReturnFocus<HTMLButtonElement>(open);

  const failed = inputOf(request.error);
  const addressError = failed === "email" && request.error ? problemMessage(request.error) : undefined;
  const passwordError = failed === "password" && request.error ? problemMessage(request.error) : undefined;

  const close = () => {
    returnFocus();
    setOpen(false);
    setAddress("");
    setPassword("");
    request.reset();
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const sent = address.trim();
    // The password is sent as typed: it can start or end with a space.
    request.mutate(
      { email: sent, current_password: password },
      {
        onSuccess: () => {
          returnFocus();
          setAsked(sent);
          setOpen(false);
          setAddress("");
          setPassword("");
        },
        onError: (error) => {
          const input = inputOf(error);
          // A password manager can fill the right one in again.
          if (error instanceof ApiProblem && error.code === "wrong_password") setPassword("");
          if (input === "email") addressInput.current?.focus();
          else if (input === "password") passwordInput.current?.focus();
        },
      },
    );
  };

  return (
    <section aria-labelledby="email" className="rounded-[14px] border border-line bg-surface p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id="email" className="text-base font-semibold">
          Email
        </h2>
        {!open && (
          <button
            ref={opener}
            type="button"
            onClick={() => {
              setAsked(null);
              setOpen(true);
            }}
            className={compactButtonClass.bordered}
          >
            Change
          </button>
        )}
      </div>
      {!open && <p className="mt-3 text-sm break-words text-ink-soft">{email}</p>}
      {asked !== null && !open && (
        <p role="status" className="mt-3 text-sm text-ink-soft">
          Check {asked}. Your address changes when you open the link. It works for a day.
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
          <div className="flex max-w-sm flex-col gap-1.5">
            <label htmlFor="new-email-input" className="text-sm font-medium text-ink">
              New address
            </label>
            <input
              ref={addressInput}
              id="new-email-input"
              type="email"
              value={address}
              onChange={(event) => {
                setAddress(event.target.value);
                if (addressError) request.reset();
              }}
              autoComplete="email"
              autoFocus
              aria-invalid={addressError ? true : undefined}
              aria-describedby={addressError ? "new-email-input-note" : undefined}
              className={`${inputClass} aria-invalid:border-overdue-ink`}
            />
            {addressError && (
              <p id="new-email-input-note" role="alert" className="text-xs font-medium text-overdue-ink">
                {addressError}
              </p>
            )}
          </div>
          <PasswordInput
            id="email-current-password-input"
            label="Current password"
            inputRef={passwordInput}
            value={password}
            onChange={(value) => {
              setPassword(value);
              if (passwordError) request.reset();
            }}
            autoComplete="current-password"
            error={passwordError}
            inputClass={inputClass}
          />
          {failed === null && <ProblemText error={request.error} className="text-overdue-ink" />}
          <div className="flex flex-wrap items-center gap-2">
            <button type="submit" disabled={request.isPending} className={compactButtonClass.primary}>
              Send link
            </button>
            <button type="button" disabled={request.isPending} onClick={close} className={compactButtonClass.quiet}>
              Cancel
            </button>
          </div>
        </form>
      )}
    </section>
  );
}
