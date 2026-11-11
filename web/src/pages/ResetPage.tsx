import { useRouter } from "@tanstack/react-router";
import { useRef, useState, type FormEvent } from "react";

import { ApiProblem, type PasswordReset } from "../api/client";
import { usePasswordReset, useResetPassword } from "../api/queries";
import { Field } from "../components/Field";
import { problemMessage } from "../components/ProblemText";
import { AuthCard, authButtonClass } from "./AuthPage";
import { Checking, Heading, Notice, Retry, useMoveFocus, type MoveFocus } from "./authStates";

/** The line the app shows once the user lands (`NoticeLine`). */
const SIGNED_IN_NOTICE = "Password changed. You're signed in.";

/** Where focus never moves to a heading: the form's input takes it. */
const KEEP_FOCUS: MoveFocus = { current: false };

/**
 * The page a password reset link opens (ux.md, Reset page), outside the app's shell like sign-in: set a new password
 * and be signed in, or learn why the link can't be used. The server decides, through the link's state; this only
 * shows it. Holding the link is the permission, so a signed-in visitor sees the same page.
 */
export function ResetPage({ token }: { token: string }) {
  const reset = usePasswordReset(token);
  const moveFocus = useMoveFocus(reset.isPending);
  const { data, error } = reset;

  if (reset.isPending) {
    return (
      <AuthCard>
        <Checking />
      </AuthCard>
    );
  }
  if (error instanceof ApiProblem && error.status === 404) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading="This isn't a reset link.">
          <p className="text-sm text-ink-soft">
            Check that the whole link was copied, or ask whoever runs this server for a new one.
          </p>
        </Notice>
      </AuthCard>
    );
  }
  if (!data) {
    // A failed refresh with the link on screen keeps it; this is only when there is none.
    return (
      <AuthCard>
        <Retry error={error} onRetry={() => void reset.refetch()} />
      </AuthCard>
    );
  }
  return (
    <AuthCard>
      {data.state === "working" ? (
        <SetPassword reset={data} token={token} />
      ) : (
        <NotWorking state={data.state} moveFocus={moveFocus} />
      )}
    </AuthCard>
  );
}

/** A link that was used, cancelled or has ended: which, and that a new one comes from whoever runs the server. */
function NotWorking({ state, moveFocus }: { state: Exclude<PasswordReset["state"], "working">; moveFocus: MoveFocus }) {
  const heading = {
    used: "This reset link was already used.",
    cancelled: "This reset link no longer works.",
    ended: "This reset link has ended.",
  }[state];
  return (
    <Notice moveFocus={moveFocus} heading={heading}>
      {state === "cancelled" && (
        <p className="text-sm text-ink-soft">A newer one was made, or the password was changed since.</p>
      )}
      <p className="text-sm text-ink-soft">Ask whoever runs this server for a new one.</p>
    </Notice>
  );
}

/** The words for a problem about the new password; an empty one is "a new password", not "your password". */
function passwordMessage(error: Error): string {
  return error instanceof ApiProblem && error.code === "field_empty" ? "Enter a new password." : problemMessage(error);
}

/** Whether a problem is about the password input, which shows it under the input. */
function isPasswordProblem(error: Error | null): error is ApiProblem {
  return (
    error instanceof ApiProblem &&
    (error.code === "password_too_short" ||
      error.code === "password_too_long" ||
      (error.code === "field_empty" && error.field === "password"))
  );
}

/**
 * The working link: the new password's input. Setting it signs the browser in as the link's user, replaces the reset
 * page in the history and lands on `/` with a toast.
 */
function SetPassword({ reset, token }: { reset: PasswordReset; token: string }) {
  const router = useRouter();
  const set = useResetPassword(token);
  const [password, setPassword] = useState("");
  const formRef = useRef<HTMLFormElement>(null);
  const { error } = set;
  const inputProblem = isPasswordProblem(error) ? error : null;
  // The link having changed shows as the page it makes, not as an error above the button.
  const changed = error instanceof ApiProblem && (error.status === 404 || error.status === 409);
  const aboveButton = error && !inputProblem && !changed ? error : null;
  // Disabled once set too, until the landing replaces the page.
  const busy = set.isPending || set.isSuccess;

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy) return;
    // Sent as typed: a password can start or end with a space.
    set.mutate(
      { password },
      {
        onSuccess: () => void router.navigate({ to: "/", replace: true, state: { notice: SIGNED_IN_NOTICE } }),
        onError: (problem) => {
          if (isPasswordProblem(problem)) {
            const input = formRef.current?.elements.namedItem("password");
            if (input instanceof HTMLInputElement) input.focus();
          }
        },
      },
    );
  };

  return (
    <form ref={formRef} onSubmit={submit} noValidate className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <Heading moveFocus={KEEP_FOCUS}>Set a new password</Heading>
        <p className="text-sm text-ink-soft">
          for {reset.display_name} (@{reset.username})
        </p>
      </div>
      <Field
        label="New password"
        name="password"
        value={password}
        onChange={(value) => {
          setPassword(value);
          // Editing the input clears its own error.
          if (inputProblem) set.reset();
        }}
        error={inputProblem ? passwordMessage(inputProblem) : undefined}
        hint="At least 10 characters."
        type="password"
        autoComplete="new-password"
        enterKeyHint="go"
        autoFocus
        required
      />
      {aboveButton && (
        <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
          {problemMessage(aboveButton)}
        </p>
      )}
      <button type="submit" disabled={busy} className={authButtonClass}>
        Set password and sign in
      </button>
      <p className="text-center text-xs text-muted">This signs you out everywhere else.</p>
    </form>
  );
}
