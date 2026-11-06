import type { ItemRef } from "../api/client";
import { STATE_LABELS } from "./labels";

/**
 * Picks the item a blocked item waits for. `candidates` are the heap's other non-closed items; the server refuses
 * a pick that would form a blocking cycle. The empty option means no blocker was picked.
 */
export function BlockerSelect({
  value,
  candidates,
  onChange,
  disabled,
}: {
  value: ItemRef | null | undefined;
  candidates: ItemRef[];
  onChange: (blockerId: string | null) => void;
  disabled?: boolean;
}) {
  // A closed blocker isn't among the candidates, but the select still has to show it.
  const options = value && !candidates.some((c) => c.id === value.id) ? [value, ...candidates] : candidates;
  return (
    <label className="inline-flex min-w-0 items-center gap-1 text-xs text-stone-500">
      <span className="shrink-0">Waiting for</span>
      <select
        value={value?.id ?? ""}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
        className="max-w-64 min-w-0 truncate rounded border border-stone-300 bg-white px-1 py-0.5 text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
      >
        <option value="">Nothing picked</option>
        {options.map((item) => (
          <option key={item.id} value={item.id}>
            {item.title}
            {item.state === "done" || item.state === "wont_do" ? ` (${STATE_LABELS[item.state].toLowerCase()})` : ""}
          </option>
        ))}
      </select>
    </label>
  );
}
