import { useId } from "react";
import { ToggleButton, ToggleButtonGroup, type Key } from "react-aria-components";

import type { Need } from "../api/client";
import { choiceButtonClass, FieldFrame, FieldLabel, type FieldLayout } from "./ChoiceButtons";
import { NEED_HINTS, NEED_LABELS, NEEDS } from "./labels";

/**
 * Picks how much an item is needed. Every item has a need, so once one is picked it can be changed but not cleared:
 * pressing the picked step again keeps it. The hint under the steps says what the picked one means, or what the
 * field asks while nothing is picked. Stacked, two steps a row on a phone, where four don't fit their names; as
 * `chips`, for the phone's Add sheet, one row that scrolls sideways, named but without a label to show.
 */
export function NeedPicker({
  value,
  onChange,
  disabled,
  layout = "stack",
}: {
  value: Need | null;
  onChange: (need: Need) => void;
  disabled?: boolean;
  layout?: FieldLayout | "chips";
}) {
  const labelId = useId();
  const hintId = useId();
  const onSelectionChange = (keys: Set<Key>) => {
    const [picked] = [...keys];
    if (picked !== undefined) onChange(String(picked) as Need);
  };
  const chips = layout === "chips";
  const groupClass = {
    stack: "grid grid-cols-2 gap-1 sm:grid-cols-4",
    grid: "grid grid-cols-4 gap-1",
    chips: "-mx-4 flex gap-1.5 overflow-x-auto px-4 [scrollbar-width:none]",
  }[layout];
  return (
    <FieldFrame
      layout={chips ? "stack" : layout}
      label={
        chips ? (
          <span id={labelId} className="sr-only">
            Need
          </span>
        ) : (
          <FieldLabel id={labelId}>Need</FieldLabel>
        )
      }
    >
      <ToggleButtonGroup
        aria-labelledby={labelId}
        selectionMode="single"
        disallowEmptySelection
        selectedKeys={value === null ? [] : [value]}
        onSelectionChange={onSelectionChange}
        isDisabled={disabled}
        className={groupClass}
      >
        {NEEDS.map((need) => (
          // On each button: the group passes no `aria-describedby` on, so a screen reader would never hear the hint.
          <ToggleButton
            key={need}
            id={need}
            aria-describedby={hintId}
            className={`${choiceButtonClass} rounded-lg ${chips ? "" : "px-1"}`}
          >
            {NEED_LABELS[need]}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
      <p id={hintId} className="text-xs text-muted">
        {value === null ? "Required: what happens if it never gets done?" : NEED_HINTS[value]}
      </p>
    </FieldFrame>
  );
}
