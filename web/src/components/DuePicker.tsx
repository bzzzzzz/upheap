import { useId, useState } from "react";
import { ToggleButton, ToggleButtonGroup, type Key } from "react-aria-components";

import type { DueInput } from "../api/client";
import { useDueChoices } from "../api/queries";
import { choiceButtonClass, FieldLabel } from "./ChoiceButtons";
import { SheetChoice } from "./Sheet";

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
  current,
}: {
  value: DueInput | null;
  onChange: (value: DueInput | null) => void;
  disabled?: boolean;
  /** How the saved due time reads now, beside the label, as a loose one picked days ago may match no choice. */
  current?: string;
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
      <div className="flex items-baseline gap-2">
        <FieldLabel id={labelId}>Due</FieldLabel>
        {current && <span className="text-xs text-muted">{current}</span>}
      </div>
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

/**
 * The due picker as a phone's sheet shows it (ux-overview.md, What comes first §3): the rough choices as 52 px rows
 * with a check on the picked one, Exact date with its date field, and Clear due. Picking a rough choice or clearing
 * is the whole job, so `onDone` closes the sheet then, also when the picked choice is tapped again; a date is typed
 * or picked in place.
 */
export function DueRows({
  value,
  onChange,
  onDone,
}: {
  value: DueInput | null;
  onChange: (value: DueInput | null) => void;
  onDone: () => void;
}) {
  const choices = useDueChoices();
  const dateId = useId();
  const noteId = useId();
  const [exact, setExact] = useState(value?.firm ?? false);
  const rough = value && !value.firm ? choices.data?.find((choice) => choice.date === value.date) : undefined;
  const picked = exact ? EXACT : (rough?.date ?? null);

  const onSelectionChange = (keys: Set<Key>) => {
    const [key] = [...keys];
    if (key === EXACT) {
      setExact(true);
      if (value && !value.firm) onChange({ date: value.date, firm: true });
      return;
    }
    // An empty selection is the picked choice tapped again: nothing to change.
    if (key !== undefined) {
      setExact(false);
      onChange({ date: String(key), firm: false });
    }
    if (key !== undefined || !exact) onDone();
  };

  return (
    <div className="flex flex-col gap-1">
      <ToggleButtonGroup
        aria-label="Due"
        selectionMode="single"
        orientation="vertical"
        selectedKeys={picked === null ? [] : [picked]}
        onSelectionChange={onSelectionChange}
        className="flex flex-col"
      >
        {(choices.data ?? []).map((choice) => (
          <SheetChoice key={choice.date} id={choice.date}>
            {/* The server's label is lower case, as it reads inside a sentence. */}
            <span className="inline-block first-letter:uppercase">{choice.label}</span>
          </SheetChoice>
        ))}
        <SheetChoice id={EXACT} note={{ id: noteId, text: "Can go overdue" }}>
          Exact date
        </SheetChoice>
      </ToggleButtonGroup>
      {exact && (
        <div className="px-3 pb-1">
          <label htmlFor={dateId} className="sr-only">
            Exact due date
          </label>
          <input
            id={dateId}
            type="date"
            value={value?.firm ? value.date : ""}
            onChange={(e) => onChange(e.target.value ? { date: e.target.value, firm: true } : null)}
            className="h-11 w-full rounded-xl border border-accent bg-surface px-3 text-base text-ink"
          />
        </div>
      )}
      {choices.isError && <span className="px-3 text-xs text-overdue-ink">The rough choices didn't load.</span>}
      <button
        type="button"
        disabled={value === null}
        onClick={() => {
          onChange(null);
          onDone();
        }}
        className="mt-1 min-h-12 rounded-xl text-[15px] text-ink-soft hover:bg-sunken disabled:opacity-40"
      >
        Clear due
      </button>
    </div>
  );
}
