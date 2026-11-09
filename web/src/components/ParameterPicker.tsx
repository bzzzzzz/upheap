import { useId, type ReactNode } from "react";

import type { ItemParameter, Parameter } from "../api/client";
import { ChoiceButtons, FieldFrame, type FieldLayout } from "./ChoiceButtons";

/** The parameters of an item that has a value, as "Name: Label" chips. */
export function ParameterChips({ parameters, className }: { parameters: ItemParameter[]; className?: string }) {
  const set = parameters.filter((parameter) => parameter.option !== null);
  if (set.length === 0) return null;
  return (
    <ul className={`flex-wrap gap-1 ${className ?? "inline-flex"}`} aria-label="Parameters">
      {set.map((parameter) => (
        <li key={parameter.parameter_id} className="rounded-md bg-param-soft px-1.5 text-xs text-param-ink">
          {parameter.name}: {parameter.option?.label}
        </li>
      ))}
    </ul>
  );
}

/** Up to this many short options show as buttons; more, or longer labels, as a menu (ux.md, The item §3). */
const MAX_BUTTONS = 4;
const MAX_BUTTON_LABEL = 12;

/**
 * Picks a value for one of the heap's parameters, as effort and impact are picked: a row of option buttons when there
 * are a few short options, or a menu otherwise. `hint`, under the control, says why it's asked.
 */
export function ParameterPicker({
  parameter,
  value,
  onChange,
  disabled,
  layout = "stack",
  hint,
}: {
  parameter: Parameter;
  value: string | null;
  onChange: (optionId: string | null) => void;
  disabled?: boolean;
  layout?: FieldLayout;
  hint?: ReactNode;
}) {
  const id = useId();
  const hintId = useId();
  const options = parameter.options;
  if (options.length <= MAX_BUTTONS && options.every((option) => option.label.length <= MAX_BUTTON_LABEL)) {
    return (
      <ChoiceButtons
        label={<span className="text-param-ink">{parameter.name}</span>}
        options={options.map((option) => ({ id: option.id, label: option.label }))}
        value={value}
        onChange={onChange}
        disabled={disabled}
        tone="param"
        layout={layout}
        hint={hint}
      />
    );
  }
  return (
    <FieldFrame
      layout={layout}
      label={
        <label htmlFor={id} className="text-xs font-medium text-param-ink">
          {parameter.name}
        </label>
      }
    >
      <select
        id={id}
        value={value ?? ""}
        disabled={disabled}
        aria-describedby={hint ? hintId : undefined}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
        className="h-9 rounded-lg border border-line bg-surface px-2 text-sm text-ink pointer-coarse:h-11"
      >
        <option value="">Not set</option>
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.label}
          </option>
        ))}
      </select>
      {hint && (
        <span id={hintId} className="text-xs text-muted">
          {hint}
        </span>
      )}
    </FieldFrame>
  );
}
