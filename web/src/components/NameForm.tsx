import { useEffect, useRef, useState, type FormEvent } from "react";

import { ProblemText } from "./ProblemText";

export const inputClass =
  "rounded border border-stone-300 bg-white px-2 py-1 text-sm text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100";
export const primaryButtonClass =
  "rounded-md bg-stone-900 px-3 py-1 text-sm font-medium text-white disabled:opacity-40 dark:bg-stone-100 dark:text-stone-900";
export const quietButtonClass =
  "rounded px-1.5 py-1 text-sm text-stone-500 hover:text-stone-900 disabled:opacity-40 dark:hover:text-stone-100";

export type NameFormClasses = { input: string; save: string; cancel: string; problem?: string };

const defaultClasses: NameFormClasses = { input: inputClass, save: primaryButtonClass, cancel: quietButtonClass };

/**
 * A name that can be edited in place: Save and Cancel appear once it changes. When the server sends a newer
 * `version` (a save, or someone else's edit), the input shows it. With `onCancel`, the form was opened to edit: Cancel
 * always shows, and it, Escape, or Enter with nothing changed closes the form.
 */
export function NameForm({
  label,
  value,
  version,
  maxLength = 100,
  pending,
  error,
  onSave,
  className = "",
  classes = defaultClasses,
  onCancel,
  autoFocus = false,
  onDraftChange,
}: {
  label: string;
  value: string;
  version: number;
  maxLength?: number;
  pending: boolean;
  error: Error | null;
  onSave: (name: string) => void;
  className?: string;
  /** The input's, buttons' and error's look, for a page in the design tokens; other pages keep the defaults. */
  classes?: NameFormClasses;
  onCancel?: () => void;
  autoFocus?: boolean;
  /** Told the text in the input whenever it changes, for a note about it beside the form, such as a new link. */
  onDraftChange?: (name: string) => void;
}) {
  const [name, setName] = useState(value);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!autoFocus) return;
    // Opened from a menu or a sheet, which gives focus back to what opened it a frame after it closes; this comes
    // after that.
    let second = 0;
    const first = window.requestAnimationFrame(() => {
      second = window.requestAnimationFrame(() => input.current?.focus());
    });
    return () => {
      window.cancelAnimationFrame(first);
      window.cancelAnimationFrame(second);
    };
  }, [autoFocus]);
  const [shownVersion, setShownVersion] = useState(version);
  if (shownVersion !== version) {
    setShownVersion(version);
    setName(value);
  }
  useEffect(() => onDraftChange?.(name), [name, onDraftChange]);
  // The server trims names, so surrounding spaces alone aren't a change.
  const changed = name.trim() !== value;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (changed && name.trim() !== "") onSave(name);
  };

  return (
    <form onSubmit={submit} className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
      <input
        ref={input}
        aria-label={label}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          // Opened to edit and left as it was: Enter closes it, as Cancel would. (It can't submit: Save is disabled.)
          if (e.key === "Enter" && !changed && onCancel) {
            e.preventDefault();
            onCancel();
            return;
          }
          if (e.key !== "Escape" || (!changed && !onCancel)) return;
          e.preventDefault();
          setName(value);
          onCancel?.();
        }}
        maxLength={maxLength}
        className={`${classes.input} min-w-0 flex-1 ${className}`}
      />
      {(changed || onCancel) && (
        <>
          <button type="submit" disabled={pending || !changed || name.trim() === ""} className={classes.save}>
            Save
          </button>
          <button
            type="button"
            onClick={() => {
              setName(value);
              onCancel?.();
            }}
            className={classes.cancel}
          >
            Cancel
          </button>
        </>
      )}
      <ProblemText error={error} className={classes.problem} />
    </form>
  );
}
