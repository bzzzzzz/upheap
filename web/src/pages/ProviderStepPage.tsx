import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { ApiProblem } from "../api/client";
import { useOidcPending, useOidcProvider, useRegisterOidc } from "../api/queries";
import { Field } from "../components/Field";
import { problemMessage } from "../components/ProblemText";
import { AuthCard, authButtonClass, REGISTER_NOTE, USERNAME_HINT } from "./AuthPage";
import { Checking, Heading, Notice, Retry, useMoveFocus, type MoveFocus } from "./authStates";

/**
 * The step after a first sign-in through the provider (ux.md, Sign in and create account): who the provider vouched
 * for, and the username to pick for the account that makes. Outside the app's shell like sign-in, with no guard: the
 * browser holds no session yet. The server decides whether the step is still open; this only shows it.
 * `onSignedIn` gets the app path the sign-in started from, if there was one.
 */
export function ProviderStepPage({ onSignedIn }: { onSignedIn: (redirect: string | undefined) => void }) {
  const pending = useOidcPending();
  const provider = useOidcProvider();
  const register = useRegisterOidc();
  const moveFocus = useMoveFocus(pending.isPending);
  const { data, error } = pending;

  const isEnded = (e: Error | null) => e instanceof ApiProblem && e.status === 404;
  // Signed in: the cache was dropped, so this page's reads start over (and would find the step gone) until it is left.
  if (register.isSuccess || pending.isPending || provider.isPending) {
    return (
      <AuthCard>
        <Checking />
      </AuthCard>
    );
  }
  if (isEnded(error) || isEnded(register.error)) {
    return (
      <AuthCard>
        <Notice
          moveFocus={moveFocus}
          heading="This step has ended."
          action={
            <Link to="/login" className={`${authButtonClass} mt-1 flex items-center justify-center`}>
              Sign in again
            </Link>
          }
        >
          <p className="text-sm text-ink-soft">Sign in again to pick your username.</p>
        </Notice>
      </AuthCard>
    );
  }
  if (!data) {
    return (
      <AuthCard>
        <Retry error={error} onRetry={() => void pending.refetch()} />
      </AuthCard>
    );
  }
  return (
    <AuthCard>
      <UsernameForm
        displayName={data.display_name}
        email={data.email}
        label={provider.data?.label}
        register={register}
        onSignedIn={onSignedIn}
        moveFocus={moveFocus}
      />
    </AuthCard>
  );
}

function UsernameForm({
  displayName,
  email,
  label,
  register,
  onSignedIn,
  moveFocus,
}: {
  displayName: string;
  email: string;
  label: string | undefined;
  register: ReturnType<typeof useRegisterOidc>;
  onSignedIn: (redirect: string | undefined) => void;
  moveFocus: MoveFocus;
}) {
  const navigate = useNavigate();
  const [username, setUsername] = useState("");
  const formRef = useRef<HTMLFormElement>(null);
  const { error } = register;
  // A problem about the username shows under it; any other, above the button.
  const usernameError = error instanceof ApiProblem && error.field === "username" ? error : undefined;

  // After a failed submit, focus goes to the input once it shows the error.
  useEffect(() => {
    const field = usernameError && formRef.current?.elements.namedItem("username");
    if (field instanceof HTMLInputElement) field.focus();
  }, [usernameError]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    register.mutate({ username }, { onSuccess: (registered) => onSignedIn(registered.redirect ?? undefined) });
  };

  return (
    // The server checks the username, so the browser's own bubbles stay out of the way.
    <form ref={formRef} onSubmit={submit} noValidate className="flex flex-col gap-4">
      <Heading moveFocus={moveFocus}>Pick a username</Heading>
      <div className="flex flex-col gap-1 text-sm">
        <p className="text-ink-soft">{label ? `Signed in through ${label} as:` : "Signed in as:"}</p>
        <p className="font-medium text-ink">{displayName}</p>
        <p className="break-words text-ink-soft">{email}</p>
      </div>
      <Field
        label="Username"
        name="username"
        value={username}
        error={usernameError ? problemMessage(usernameError) : undefined}
        onChange={(value) => {
          setUsername(value);
          // Editing the input clears its own error.
          if (usernameError) register.reset();
        }}
        hint={USERNAME_HINT}
        autoComplete="username"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        maxLength={39}
        autoFocus
        required
      />
      {error && !usernameError && (
        <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
          {problemMessage(error)}
        </p>
      )}
      <button type="submit" disabled={register.isPending} className={authButtonClass}>
        Create account
      </button>
      <button
        type="button"
        onClick={() => void navigate({ to: "/login" })}
        className="self-center rounded-lg px-2 py-1.5 text-sm font-semibold text-accent-ink underline pointer-coarse:min-h-11"
      >
        Cancel
      </button>
      <p className="text-center text-xs text-muted">{REGISTER_NOTE}</p>
    </form>
  );
}
