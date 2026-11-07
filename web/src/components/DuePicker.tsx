import { useId, useState } from "react";
import { ToggleButton, ToggleButtonGroup, type Key } from "react-aria-components";

import type { DueInput } from "../api/client";
import { useDueChoices } from "../api/queries";
import { choiceButtonClass, FieldLabel } from "./ChoiceButtons";

const EXACT = "exact";

/**
 * Picks a due time (app-ux.md, Due): the server's rough choices first, each a loose due time, and Exact date last,
 * the only one that opens a date field and the only one that can go overdue. Pressing the picked choice again clears
 * the due time.
 */
export function DuePicker({
  value,
  onChange,
  disabled,
}: {
  value: DueInput | null;
  onChange: (value: DueInput | null) => void;
  disabled?: boolean;
}) {
  const choices = useDueChoices();
  const labelId = useId();
  const dateId = useId();
  // Exact date stays picked while its field is still empty, before there's a due time to send.
  const [exact, setExact] = useState(value?.firm ?? false);
  const rough = value && !value.firm ? choices.data?.find((choice) => choice.date === value.date) : undefined;
  const picked = exact ? EXACT : (rough?.date ?? null);

  const onSelectionChange = (keys: Set<Key>) => {
    const [key] = [...keys];
    if (key === EXACT) {
      setExact(true);
      onChange(value ? { date: value.date, firm: true } : null);
      return;
    }
    setExact(false);
    onChange(key === undefined ? null : { date: String(key), firm: false });
  };

  return (
    <div className="flex flex-col gap-1.5">
      <FieldLabel id={labelId}>Due</FieldLabel>
      <ToggleButtonGroup
        aria-labelledby={labelId}
        selectionMode="single"
        selectedKeys={picked === null ? [] : [picked]}
        onSelectionChange={onSelectionChange}
        isDisabled={disabled}
        className="flex flex-wrap gap-1.5"
      >
        {(choices.data ?? []).map((choice) => (
          <ToggleButton key={choice.date} id={choice.date} className={`${choiceButtonClass} rounded-full`}>
            {/* The server's label is lower case, as it reads inside a sentence. */}
            <span className="inline-block first-letter:uppercase">{choice.label}</span>
          </ToggleButton>
        ))}
        <ToggleButton id={EXACT} className={`${choiceButtonClass} rounded-full`}>
          Exact date
        </ToggleButton>
      </ToggleButtonGroup>
      {exact && (
        <div className="flex flex-col gap-1">
          <label htmlFor={dateId} className="sr-only">
            Exact due date
          </label>
          <input
            id={dateId}
            type="date"
            value={value?.firm ? value.date : ""}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value ? { date: e.target.value, firm: true } : null)}
            className="h-9 w-full max-w-56 rounded-lg border border-line bg-surface px-2 text-sm text-ink pointer-coarse:h-11"
          />
          <span className="text-xs text-muted">An exact date can become overdue.</span>
        </div>
      )}
      {choices.isError && <span className="text-xs text-overdue-ink">The rough choices didn't load.</span>}
    </div>
  );
}
