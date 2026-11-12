import { useId, useState } from "react";

import { FieldFrame, FieldLabel, type FieldLayout } from "./ChoiceButtons";

/**
 * A reminder's date: an exact calendar date and nothing else, `YYYY-MM-DD` (ux.md, Adding an item §5; The item §2).
 * It is the reminder's deadline, so there are no rough choices and it can't be cleared: an empty field reports
 * nothing, and the saved date stays until another one is picked. `wide` is the phone's sheet, with its larger field.
 */
export function DateField({
  value,
  onChange,
  disabled,
  layout = "stack",
  label = "Date",
  wide = false,
}: {
  value: string | null;
  onChange: (date: string) => void;
  disabled?: boolean;
  layout?: FieldLayout;
  label?: string;
  wide?: boolean;
}) {
  const labelId = useId();
  const inputId = useId();
  // What the field shows while a date is typed in pieces; it follows the saved date whenever that changes.
  const [shown, setShown] = useState({ value, text: value ?? "" });
  if (shown.value !== value) setShown({ value, text: value ?? "" });

  return (
    <FieldFrame
      layout={layout}
      label={
        <label id={labelId} htmlFor={inputId} className="contents">
          <FieldLabel id={`${labelId}-text`}>{label}</FieldLabel>
        </label>
      }
    >
      <input
        id={inputId}
        type="date"
        value={shown.text}
        disabled={disabled}
        onChange={(e) => {
          setShown({ value, text: e.target.value });
          if (e.target.value && e.target.value !== value) onChange(e.target.value);
        }}
        className={
          wide
            ? "h-11 w-full rounded-xl border border-accent bg-surface px-3 text-base text-ink"
            : "h-9 w-full max-w-56 rounded-lg border border-line bg-surface px-2 text-sm text-ink disabled:opacity-60 pointer-coarse:h-11"
        }
      />
    </FieldFrame>
  );
}
