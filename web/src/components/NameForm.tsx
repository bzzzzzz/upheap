import { useEffect, useRef, useState, type FormEvent } from "react";

import { ProblemText } from "./ProblemText";

export type NameFormClasses = { input: string; save: string; cancel: string; problem?: string };

/**
 * A name that can be edited in place: Save and Cancel appear once it changes. When the saved `value` changes (a save,
 * or someone else's edit), the input shows it. With `onCancel`, the form was opened to edit: Cancel
 * always shows, and it, Escape, or Enter with nothing changed closes the form.
 */
export function NameForm({
  label,
  value,
  maxLength = 100,
  pending,
  error,
  onSave,
  className = "",
  classes,
  onCancel,
  autoFocus = false,
  onDraftChange,
}: {
  label: string;
  value: string;
  maxLength?: number;
  pending: boolean;
  error: Error | null;
  onSave: (name: string) => void;
  className?: string;
  /** The input's, buttons' and error's look, which each page sets. */
  classes: NameFormClasses;
  onCancel?: () => void;
  autoFocus?: boolean;
  /** Told the text in the input whenever it changes, for a note about it beside the form, such as a new link. */
  onDraftChange?: (name: string) => void;
}) {
  const [name, setName] = useState(value);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    // The form mounts in the render that closes the menu or sheet it was opened from. A passive effect runs after
    // that overlay has lifted the page's inert and before its focus restore, which then leaves the input focused
    // (useFocusAfterRender explains why).
    if (autoFocus) input.current?.focus();
  }, [autoFocus]);
  // Reseeded when the saved value changes, not on every new version, so saving another field beside this one, which
  // bumps the record's version, doesn't drop a name being typed here.
  const [shownValue, setShownValue] = useState(value);
  if (shownValue !== value) {
    setShownValue(value);
    setName(value);
  }
  useEffect(() => onDraftChange?.(name), [name, onDraftChange]);
  // The server trims names, so surrounding spaces alone aren't a change.
  const changed = name.trim() !== value;

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!changed || name.trim() === "") return;
    // Save goes once the name is saved, so focus that's on it waits in the input rather than falling to the page. Not
    // for a finger, where focusing the input brings the on-screen keyboard back up just after Save was tapped.
    if (event.currentTarget.contains(document.activeElement) && !window.matchMedia("(pointer: coarse)").matches) {
      input.current?.focus();
    }
    onSave(name);
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
