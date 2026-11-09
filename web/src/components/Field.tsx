import { useId, type InputHTMLAttributes } from "react";

/**
 * A labelled input with a hint or an error under it, in the first-set sizes (WideSignIn, WideTeams): 48 px tall with
 * 16 px text on a phone, compact on a wider screen. The error is an alert, so it's announced also when focus is already
 * in the input, as after submitting with Enter.
 */
export function Field({
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
