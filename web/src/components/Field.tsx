import { useId, type InputHTMLAttributes } from "react";

/** A labelled input, with `hint` below it when given. */
export function Field({
  label,
  hint,
  ...input
}: { label: string; hint?: string } & InputHTMLAttributes<HTMLInputElement>) {
  const id = useId();
  const hintId = useId();
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-sm font-medium">
        {label}
      </label>
      <input
        id={id}
        {...input}
        aria-describedby={hint ? hintId : undefined}
        className="rounded-md border border-stone-300 bg-white px-3 py-2 dark:border-stone-700 dark:bg-stone-900"
      />
      {hint && (
        <p id={hintId} className="text-xs text-stone-500">
          {hint}
        </p>
      )}
    </div>
  );
}
