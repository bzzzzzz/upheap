import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Tab, TabList, TabPanel, Tabs, type Key } from "react-aria-components";

import { ApiProblem } from "../api/client";
import { useRegister, useSignIn } from "../api/queries";
import { Field } from "../components/Field";
import { Logo } from "../components/Logo";
import { problemMessage } from "../components/ProblemText";

export type Mode = "sign_in" | "register";
/** The form's inputs, named as the API names them, so a problem's `field` says which one it's about. */
type FieldName = "display_name" | "username" | "email" | "login" | "password";
type Values = Record<FieldName, string>;

const FIELDS: Record<Mode, readonly FieldName[]> = {
  sign_in: ["login", "password"],
  register: ["display_name", "username", "email", "password"],
};

/** What Create account sends. */
type RegisterBody = { email: string; username: string; display_name: string; password: string };

/** A request that creates an account: `useRegister`'s shape, which the invite page replaces with its own. */
export type RegisterRequest = {
  mutate: (body: RegisterBody, options: { onSuccess: () => void }) => void;
  error: Error | null;
  isPending: boolean;
  reset: () => void;
};

/**
 * Sign in, or create an account (S-sign-in, S-register): a card on a wide screen (WideSignIn), the bare form on a
 * phone (PhoneSignIn). `notice` says where a link the user followed while signed out leads.
 *
 * The invite page (ux.md, Invite page) uses the same switch and forms, changed through the optional props: `header`
 * above the switch, `initialMode` (Create account comes first in the switch when it starts selected), its own
 * `register` request, and Create account's submit label (`registerLabel`) and footnote (`registerNote`).
 */
export function AuthPage({
  notice,
  header,
  initialMode = "sign_in",
  register: registerOverride,
  registerLabel = "Create account",
  registerNote = "You'll start in your own heap. Teams come next.",
  onSignedIn,
}: {
  notice?: string;
  header?: ReactNode;
  initialMode?: Mode;
  register?: RegisterRequest;
  registerLabel?: string;
  registerNote?: string;
  onSignedIn: () => void;
}) {
  const [mode, setMode] = useState<Mode>(initialMode);
  // After "Sign in instead", the password is the next thing to type: the address is already carried over.
  const [focusPassword, setFocusPassword] = useState(false);
  // Kept across the switch, so an email typed under Sign in stays for Create account; the password isn't.
  const [values, setValues] = useState<Values>({
    display_name: "",
    username: "",
    email: "",
    login: "",
    password: "",
  });
  // Here rather than in the form, which the switch unmounts: a submit's navigation must outlive it.
  const signIn = useSignIn();
  const defaultRegister = useRegister();
  const register = registerOverride ?? defaultRegister;
  const pending = signIn.isPending || register.isPending;
  const selectMode = (key: Key) => {
    setMode(key === "register" ? "register" : "sign_in");
    // Each form starts clean. A sign-in password, perhaps mistyped, mustn't become the new one.
    // An email address carries over between Sign in's "Email or username" and Create account's Email.
    setValues((current) => ({
      ...current,
      password: "",
      ...(key === "register" && current.email === "" && current.login.includes("@") && { email: current.login }),
      ...(key === "sign_in" && current.login === "" && { login: current.email }),
    }));
    signIn.reset();
    register.reset();
  };
  const form = (formMode: Mode) => (
    <AuthForm
      mode={formMode}
      mutation={formMode === "sign_in" ? signIn : register}
      submitLabel={formMode === "register" ? registerLabel : undefined}
      note={formMode === "register" ? registerNote : undefined}
      focusPassword={focusPassword}
      onSignInInstead={() => {
        setFocusPassword(true);
        selectMode("sign_in");
      }}
      onSubmit={() =>
        formMode === "sign_in"
          ? signIn.mutate({ login: values.login, password: values.password }, { onSuccess: onSignedIn })
          : register.mutate(
              {
                display_name: values.display_name,
                username: values.username,
                email: values.email,
                password: values.password,
              },
              { onSuccess: onSignedIn },
            )
      }
      values={values}
      onChange={(name, value) => setValues((current) => ({ ...current, [name]: value }))}
    />
  );

  return (
    <AuthCard>
      {header}
      {notice && <p className="rounded-[10px] bg-accent-soft px-3 py-2.5 text-sm text-accent-ink">{notice}</p>}
      {/* Tabs, as the switch picks which form shows below it, and arrow keys move between the two. */}
      {/* Held while a submit is pending, so its result lands on the form that sent it. */}
      <Tabs
        selectedKey={mode}
        isDisabled={pending}
        onSelectionChange={(key) => {
          setFocusPassword(false);
          selectMode(key);
        }}
      >
        <TabList
          aria-label="Sign in or create account"
          className="flex gap-1 rounded-xl bg-sunken p-[3px] sm:rounded-[10px]"
        >
          {/* A link that invites someone to a team opens with Create account, so it comes first. */}
          {initialMode === "register" ? [registerTab, signInTab] : [signInTab, registerTab]}
        </TabList>
        <TabPanel id="sign_in" className="mt-4">
          {form("sign_in")}
        </TabPanel>
        <TabPanel id="register" className="mt-4">
          {form("register")}
        </TabPanel>
      </Tabs>
    </AuthCard>
  );
}

/**
 * The page sign-in and its neighbours (connecting the CLI) sit on: a 380 px card under the Upheap mark on a wide
 * screen, the bare page on a phone.
 */
export function AuthCard({ children }: { children: ReactNode }) {
  return (
    <main className="min-h-dvh bg-surface px-5 pt-[72px] pb-4 sm:bg-canvas sm:px-4 sm:pt-16">
      <div className="flex flex-col gap-4 sm:mx-auto sm:w-[380px] sm:rounded-2xl sm:border sm:border-line sm:bg-surface sm:p-7">
        <h1 className="flex items-center gap-2 text-lg font-semibold sm:text-base">
          <Logo className="size-6 text-accent sm:size-[22px]" />
          Upheap
        </h1>
        {children}
      </div>
    </main>
  );
}

/** The card's main button: 52 px on a phone, 44 px beyond. A link styled as one adds `flex items-center justify-center`. */
export const authButtonClass =
  "h-[52px] w-full rounded-[14px] bg-ink text-base font-semibold text-surface disabled:opacity-40 sm:h-11 sm:rounded-[10px] sm:text-sm";

const tabClass =
  "flex h-10 flex-1 cursor-default items-center justify-center rounded-[10px] text-sm text-ink-soft sm:h-auto sm:rounded-lg sm:p-[7px] data-selected:bg-surface data-selected:font-semibold data-selected:text-ink data-selected:shadow-[0_1px_2px_rgba(0,0,0,0.08)]";

const signInTab = (
  <Tab key="sign_in" id="sign_in" className={tabClass}>
    Sign in
  </Tab>
);
const registerTab = (
  <Tab key="register" id="register" className={tabClass}>
    Create account
  </Tab>
);

function AuthForm({
  mode,
  mutation,
  submitLabel,
  note,
  focusPassword,
  onSignInInstead,
  onSubmit,
  values,
  onChange,
}: {
  mode: Mode;
  /** The form's request: its error and whether it's pending. */
  mutation: { error: Error | null; isPending: boolean; reset: () => void };
  /** What Create account's button says; the default is "Create account". */
  submitLabel?: string | undefined;
  /** The footnote under Create account's button. */
  note?: string | undefined;
  /** Whether Password takes focus as the form shows, as after "Sign in instead". */
  focusPassword: boolean;
  /** Switches to Sign in, the email carried over: what a taken email offers. */
  onSignInInstead: () => void;
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

  // Once, as the form shows after "Sign in instead".
  useEffect(() => {
    if (!focusPassword) return;
    const input = formRef.current?.elements.namedItem("password");
    if (input instanceof HTMLInputElement) input.focus();
  }, [focusPassword]);

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
        <Field label="Name" {...field("display_name")} autoComplete="name" autoCapitalize="words" required />
      )}
      {mode === "register" && (
        <Field
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
      {mode === "sign_in" ? (
        <Field
          label="Email or username"
          {...field("login")}
          autoComplete="username"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          required
        />
      ) : (
        <Field
          label="Email"
          {...field("email")}
          type="email"
          autoComplete="email"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          required
        />
      )}
      {mode === "register" && errorField === "email" && error instanceof ApiProblem && error.code === "email_taken" && (
        <button
          type="button"
          onClick={onSignInInstead}
          className="-mt-2 self-start rounded-lg px-1 py-1.5 text-sm font-semibold text-accent-ink underline pointer-coarse:min-h-11"
        >
          Sign in instead
        </button>
      )}
      <Field
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
      <button type="submit" disabled={mutation.isPending} className={authButtonClass}>
        {mode === "sign_in" ? "Sign in" : (submitLabel ?? "Create account")}
      </button>
      {mode === "register" && note && <p className="text-center text-xs text-muted">{note}</p>}
    </form>
  );
}
