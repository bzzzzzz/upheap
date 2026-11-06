import { useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

import { useRegister, useSignIn } from "../api/queries";
import { Field } from "../components/Field";

export function AuthPage() {
  const [mode, setMode] = useState<"sign_in" | "register">("sign_in");
  const signIn = useSignIn();
  const register = useRegister();
  const navigate = useNavigate();
  const pending = signIn.isPending || register.isPending;
  const error = mode === "sign_in" ? signIn.error : register.error;

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const email = String(form.get("email") ?? "");
    const password = String(form.get("password") ?? "");
    const done = { onSuccess: () => void navigate({ to: "/" }) };
    if (mode === "sign_in") {
      signIn.mutate({ email, password }, done);
    } else {
      register.mutate({ email, password, display_name: String(form.get("display_name") ?? "") }, done);
    }
  };

  return (
    <main className="mx-auto flex min-h-screen max-w-sm flex-col justify-center px-4">
      <h1 className="mb-1 text-2xl font-semibold tracking-tight">Upheap</h1>
      <p className="mb-8 text-sm text-stone-500">Add what comes up. The list keeps itself in order.</p>
      <form onSubmit={submit} className="flex flex-col gap-4" aria-describedby={error ? "auth-error" : undefined}>
        {mode === "register" && <Field label="Name" name="display_name" autoComplete="name" required />}
        <Field label="Email" name="email" type="email" autoComplete="email" required />
        <Field
          label="Password"
          name="password"
          type="password"
          autoComplete={mode === "register" ? "new-password" : "current-password"}
          minLength={mode === "register" ? 10 : undefined}
          required
        />
        {error && (
          <p id="auth-error" role="alert" className="text-sm text-red-700 dark:text-red-400">
            {error.message}
          </p>
        )}
        <button
          type="submit"
          disabled={pending}
          className="rounded-md bg-stone-900 px-4 py-2 font-medium text-white hover:bg-stone-700 disabled:opacity-50 dark:bg-stone-100 dark:text-stone-900 dark:hover:bg-stone-300"
        >
          {mode === "sign_in" ? "Sign in" : "Create account"}
        </button>
      </form>
      <button
        type="button"
        onClick={() => setMode(mode === "sign_in" ? "register" : "sign_in")}
        className="mt-4 text-sm text-stone-600 underline-offset-2 hover:underline dark:text-stone-400"
      >
        {mode === "sign_in" ? "New here? Create an account" : "Have an account? Sign in"}
      </button>
    </main>
  );
}
