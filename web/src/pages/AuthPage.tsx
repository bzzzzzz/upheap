import { useEffect, useId, useRef, useState, type FormEvent, type InputHTMLAttributes } from "react";
import { Tab, TabList, TabPanel, Tabs } from "react-aria-components";

import { ApiProblem } from "../api/client";
import { useRegister, useSignIn } from "../api/queries";
import { problemMessage } from "../components/ProblemText";

type Mode = "sign_in" | "register";
/** The form's inputs, named as the API names them, so a problem's `field` says which one it's about. */
type FieldName = "display_name" | "username" | "email" | "password";
type Values = Record<FieldName, string>;

const FIELDS: Record<Mode, readonly FieldName[]> = {
  sign_in: ["email", "password"],
  register: ["display_name", "username", "email", "password"],
};

/**
 * Sign in, or create an account (S-sign-in, S-register): a card on a wide screen (WideSignIn), the bare form on a
 * phone (PhoneSignIn). `notice` says where a link the user followed while signed out leads.
 */
export function AuthPage({ notice, onSignedIn }: { notice?: string; onSignedIn: () => void }) {
  const [mode, setMode] = useState<Mode>("sign_in");
  // Kept across the switch, so an email typed under Sign in stays for Create account; the password isn't.
  const [values, setValues] = useState<Values>({ display_name: "", username: "", email: "", password: "" });
  // Here rather than in the form, which the switch unmounts: a submit's navigation must outlive it.
  const signIn = useSignIn();
  const register = useRegister();
  const pending = signIn.isPending || register.isPending;
  const form = (formMode: Mode) => (
    <AuthForm
      mode={formMode}
      mutation={formMode === "sign_in" ? signIn : register}
      onSubmit={() =>
        formMode === "sign_in"
          ? signIn.mutate({ email: values.email, password: values.password }, { onSuccess: onSignedIn })
          : register.mutate(values, { onSuccess: onSignedIn })
      }
      values={values}
      onChange={(name, value) => setValues((current) => ({ ...current, [name]: value }))}
    />
  );

  return (
    <main className="min-h-dvh bg-surface px-5 pt-[72px] pb-4 sm:bg-canvas sm:px-4 sm:pt-16">
      <div className="flex flex-col gap-4 sm:mx-auto sm:w-[380px] sm:rounded-2xl sm:border sm:border-line sm:bg-surface sm:p-7">
        <h1 className="flex items-center gap-2 text-lg font-semibold sm:text-base">
          <UpheapMark />
          Upheap
        </h1>
        {notice && <p className="rounded-[10px] bg-accent-soft px-3 py-2.5 text-sm text-accent-ink">{notice}</p>}
        {/* Tabs, as the switch picks which form shows below it, and arrow keys move between the two. */}
        {/* Held while a submit is pending, so its result lands on the form that sent it. */}
        <Tabs
          selectedKey={mode}
          isDisabled={pending}
          onSelectionChange={(key) => {
            setMode(key === "register" ? "register" : "sign_in");
            // Each form starts clean. A sign-in password, perhaps mistyped, mustn't become the new one.
            setValues((current) => ({ ...current, password: "" }));
            signIn.reset();
            register.reset();
          }}
        >
          <TabList
            aria-label="Sign in or create account"
            className="flex gap-1 rounded-xl bg-sunken p-[3px] sm:rounded-[10px]"
          >
            <Tab id="sign_in" className={tabClass}>
              Sign in
            </Tab>
            <Tab id="register" className={tabClass}>
              Create account
            </Tab>
          </TabList>
          <TabPanel id="sign_in" className="mt-4">
            {form("sign_in")}
          </TabPanel>
          <TabPanel id="register" className="mt-4">
            {form("register")}
          </TabPanel>
        </Tabs>
      </div>
    </main>
  );
}

const tabClass =
  "flex h-10 flex-1 cursor-default items-center justify-center rounded-[10px] text-sm text-ink-soft sm:h-auto sm:rounded-lg sm:p-[7px] data-selected:bg-surface data-selected:font-semibold data-selected:text-ink data-selected:shadow-[0_1px_2px_rgba(0,0,0,0.08)]";

/** The Upheap mark: a heap of lines, narrowing to the top. */
function UpheapMark() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-6 text-accent sm:size-[22px]"
    >
      <path d="M4 18h16M6 13h12M9 8h6M11 4h2" />
    </svg>
  );
}

function AuthForm({
  mode,
  mutation,
  onSubmit,
  values,
  onChange,
}: {
  mode: Mode;
  /** The form's request: its error and whether it's pending. */
  mutation: { error: Error | null; isPending: boolean; reset: () => void };
  onSubmit: () => void;
  values: Values;
  onChange: (name: FieldName, value: string) => void;
}) {
  const { error } = mutation;
  // A problem about one of this form's inputs shows under it; any other, above the button.
  const errorField = error instanceof ApiProblem ? FIELDS[mode].find((name) => name === error.field) : undefined;
  const formRef = useRef<HTMLFormElement>(null);

  // After a failed submit, focus goes to the input the problem is about, once it shows the error.
  useEffect(() => {
    const input = errorField && formRef.current?.elements.namedItem(errorField);
    if (input instanceof HTMLInputElement) input.focus();
  }, [error, errorField]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSubmit();
  };

  const field = (name: FieldName) => ({
    name,
    value: values[name],
    error: errorField === name && error ? problemMessage(error) : undefined,
    onChange: (value: string) => {
      onChange(name, value);
      // Editing the input clears its own error.
      if (errorField === name) mutation.reset();
    },
  });

  return (
    // The server checks every input and says which one is wrong, so the browser's own bubbles stay out of the way.
    <form ref={formRef} onSubmit={submit} noValidate className="flex flex-col gap-4">
      {mode === "register" && (
        <AuthField label="Name" {...field("display_name")} autoComplete="name" autoCapitalize="words" required />
      )}
      {mode === "register" && (
        <AuthField
          label="Username"
          {...field("username")}
          hint="Lowercase letters, digits and dashes. It's in your links, and can't be changed later."
          autoComplete="username"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          maxLength={39}
          required
        />
      )}
      <AuthField
        label="Email"
        {...field("email")}
        type="email"
        autoComplete="email"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        required
      />
      <AuthField
        label="Password"
        {...field("password")}
        hint={mode === "register" ? "At least 10 characters" : undefined}
        type="password"
        autoComplete={mode === "register" ? "new-password" : "current-password"}
        enterKeyHint="go"
        required
      />
      {error && !errorField && (
        <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
          {problemMessage(error)}
        </p>
      )}
      <button
        type="submit"
        disabled={mutation.isPending}
        className="h-[52px] w-full rounded-[14px] bg-ink text-base font-semibold text-surface disabled:opacity-40 sm:h-11 sm:rounded-[10px] sm:text-sm"
      >
        {mode === "sign_in" ? "Sign in" : "Create account"}
      </button>
      {mode === "register" && (
        <p className="text-center text-xs text-muted">You'll start in your own heap. Teams come next.</p>
      )}
    </form>
  );
}

/**
 * A labelled input with a hint or an error under it, in the first-set sizes. The error is an alert, so it's announced
 * also when focus is already in the input, as after submitting with Enter.
 */
function AuthField({
  label,
  hint,
  error,
  onChange,
  ...input
}: {
  label: string;
  hint?: string;
  error?: string;
  onChange: (value: string) => void;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "onChange">) {
  const id = useId();
  const noteId = useId();
  const note = error ?? hint;
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-sm font-medium text-ink-soft sm:text-[13px]">
        {label}
      </label>
      <input
        id={id}
        {...input}
        onChange={(event) => onChange(event.target.value)}
        aria-invalid={error ? true : undefined}
        aria-describedby={note ? noteId : undefined}
        className="h-12 rounded-xl border border-line-strong bg-surface px-3 text-base text-ink aria-invalid:border-overdue-ink aria-invalid:ring-[3px] aria-invalid:ring-overdue-soft aria-invalid:outline-overdue-ink sm:h-auto sm:rounded-[9px] sm:py-[9px]"
      />
      {error ? (
        <p key="error" id={noteId} role="alert" className="text-xs font-medium text-overdue-ink">
          {error}
        </p>
      ) : (
        hint && (
          <p key="hint" id={noteId} className="text-xs text-muted">
            {hint}
          </p>
        )
      )}
    </div>
  );
}
