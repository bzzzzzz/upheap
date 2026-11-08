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
 * Where a field's label goes: above its control, as the item pane has it, or beside it, as the Add dialog has it on a
 * tablet or wide screen. There the field's label and control are the two cells of a row of the dialog's grid.
 */
export type FieldLayout = "stack" | "grid";

/** A field's label and its control, laid out as `layout` says. */
export function FieldFrame({
  layout = "stack",
  label,
  children,
}: {
  layout?: FieldLayout;
  label: ReactNode;
  children: ReactNode;
}) {
  if (layout === "grid") {
    return (
      <>
        {/* Level with the first line of buttons beside it: 36 px tall with a mouse, 44 px on a touch screen. */}
        <div className="flex pt-2.5 pointer-coarse:pt-3.5">{label}</div>
        <div className="relative flex min-w-0 flex-col gap-1.5">{children}</div>
      </>
    );
  }
  return (
    <div className="relative flex min-w-0 flex-col gap-1.5">
      {label}
      {children}
    </div>
  );
}

/**
 * One value out of a few, as a row of buttons: effort and impact sizes, a parameter's options. Pressing the picked
 * button again clears it, so "unset" needs no button of its own. `hint` goes under the buttons.
 */
export function ChoiceButtons<T extends string>({
  label,
  options,
  value,
  onChange,
  disabled,
  tone = "ink",
  layout = "stack",
  hint,
}: {
  label: ReactNode;
  options: readonly { id: T; label: string }[];
  value: T | null;
  onChange: (value: T | null) => void;
  disabled?: boolean;
  /** Violet for custom parameters, which read apart from the built-in fields everywhere. */
  tone?: "ink" | "param";
  layout?: FieldLayout;
  hint?: ReactNode;
}) {
  const labelId = useId();
  const hintId = useId();
  const onSelectionChange = (keys: Set<Key>) => {
    const [picked] = [...keys];
    onChange(picked === undefined ? null : (String(picked) as T));
  };
  const toneClass =
    tone === "param"
      ? " data-[selected]:border-param-ink data-[selected]:bg-param-soft data-[selected]:text-param-ink data-[selected]:font-medium"
      : "";
  // Stacked, the buttons share the width of the field; beside their label, each is as wide as its name.
  const grid = layout === "grid";
  return (
    <FieldFrame layout={layout} label={<FieldLabel id={labelId}>{label}</FieldLabel>}>
      <ToggleButtonGroup
        aria-labelledby={labelId}
        selectionMode="single"
        selectedKeys={value === null ? [] : [value]}
        onSelectionChange={onSelectionChange}
        isDisabled={disabled}
        className={grid ? "flex flex-wrap gap-1" : "grid auto-cols-fr grid-flow-col gap-1"}
      >
        {options.map((option) => (
          <ToggleButton
            key={option.id}
            id={option.id}
            // On each button: the group passes no `aria-describedby` on.
            aria-describedby={hint ? hintId : undefined}
            className={`${choiceButtonClass} rounded-lg ${grid ? "min-w-11" : "px-1"}${toneClass}`}
          >
            {option.label}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
      {hint && (
        <span id={hintId} className="text-xs text-muted">
          {hint}
        </span>
      )}
    </FieldFrame>
  );
}
