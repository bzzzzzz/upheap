import { useId, type ReactNode } from "react";
import { ToggleButton, ToggleButtonGroup, type Key } from "react-aria-components";

/**
 * A pickable button: outlined, filled with ink once picked. 44 px tall on touch screens, 36 px with a mouse. Without a
 * corner radius, so each use picks its own.
 */
export const choiceButtonClass =
  "flex h-9 shrink-0 cursor-default items-center justify-center border border-line bg-surface px-3 text-sm text-ink outline-none pointer-coarse:h-11 data-[disabled]:opacity-50 data-[focus-visible]:outline-2 data-[focus-visible]:outline-offset-2 data-[focus-visible]:outline-accent data-[hovered]:border-muted data-[selected]:border-ink data-[selected]:bg-ink data-[selected]:text-canvas";

/** A labelled field above its control, as the Add dialog and the item pane lay them out. */
export function FieldLabel({ id, children }: { id: string; children: ReactNode }) {
  return (
    <span id={id} className="text-xs font-medium text-ink-soft">
      {children}
    </span>
  );
}

/**
 * One value out of a few, as a row of buttons: effort and impact sizes, a parameter's options. Pressing the picked
 * button again clears it, so "unset" needs no button of its own.
 */
export function ChoiceButtons<T extends string>({
  label,
  options,
  value,
  onChange,
  disabled,
  tone = "ink",
}: {
  label: ReactNode;
  options: readonly { id: T; label: string }[];
  value: T | null;
  onChange: (value: T | null) => void;
  disabled?: boolean;
  /** Violet for custom parameters, which read apart from the built-in fields everywhere. */
  tone?: "ink" | "param";
}) {
  const labelId = useId();
  const onSelectionChange = (keys: Set<Key>) => {
    const [picked] = [...keys];
    onChange(picked === undefined ? null : (String(picked) as T));
  };
  const toneClass =
    tone === "param"
      ? " data-[selected]:border-param-ink data-[selected]:bg-param-soft data-[selected]:text-param-ink data-[selected]:font-medium"
      : "";
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <FieldLabel id={labelId}>{label}</FieldLabel>
      <ToggleButtonGroup
        aria-labelledby={labelId}
        selectionMode="single"
        selectedKeys={value === null ? [] : [value]}
        onSelectionChange={onSelectionChange}
        isDisabled={disabled}
        className="grid auto-cols-fr grid-flow-col gap-1"
      >
        {options.map((option) => (
          <ToggleButton key={option.id} id={option.id} className={`${choiceButtonClass} rounded-lg px-1${toneClass}`}>
            {option.label}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </div>
  );
}
