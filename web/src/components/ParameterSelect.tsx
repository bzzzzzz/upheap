import type { ItemParameter, Parameter } from "../api/client";

/**
 * Picks an item's option for one custom parameter. The empty option means unset. An option the item keeps after it
 * was archived (closed items keep theirs) is listed too, marked, so the select still shows the saved value.
 */
export function ParameterSelect({
  parameter,
  current,
  value,
  onChange,
  disabled,
}: {
  /** The parameter as the heap lists it, or `undefined` when it's archived. */
  parameter: Parameter | undefined;
  /** The item's value as the server sent it, for its name and an archived option. */
  current: ItemParameter;
  value: string | null;
  onChange: (optionId: string | null) => void;
  disabled?: boolean;
}) {
  const options = parameter?.options ?? [];
  const keptArchived = current.option !== null && !options.some((option) => option.id === current.option?.id);
  return (
    <label className="inline-flex items-center gap-1 text-xs text-stone-500">
      <span>
        {current.name}
        {current.parameter_archived && " (archived)"}
      </span>
      <select
        value={value ?? ""}
        disabled={disabled || parameter === undefined}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
        className="rounded border border-stone-300 bg-white px-1 py-0.5 text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
      >
        <option value="">–</option>
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.label}
          </option>
        ))}
        {keptArchived && current.option && (
          <option value={current.option.id} disabled>
            {current.option.label} (archived)
          </option>
        )}
      </select>
    </label>
  );
}

/** The parameters of an item that has a value, as "Name: Label" chips. */
export function ParameterChips({ parameters }: { parameters: ItemParameter[] }) {
  const set = parameters.filter((parameter) => parameter.option !== null);
  if (set.length === 0) return null;
  return (
    <ul className="inline-flex flex-wrap gap-1" aria-label="Parameters">
      {set.map((parameter) => (
        <li
          key={parameter.parameter_id}
          className="rounded bg-violet-100 px-1.5 text-xs text-violet-900 dark:bg-violet-950 dark:text-violet-200"
        >
          {parameter.name}: {parameter.option?.label}
        </li>
      ))}
    </ul>
  );
}
