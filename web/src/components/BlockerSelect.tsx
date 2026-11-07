import type { ItemRef } from "../api/client";
import { isClosed, STATE_LABELS } from "./labels";

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
    <label className="flex min-w-0 flex-col gap-1.5">
      <span className="text-xs font-medium text-ink-soft">Waiting for</span>
      <select
        value={value?.id ?? ""}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
        className="h-9 min-w-0 truncate rounded-lg border border-line bg-surface px-2 text-sm text-ink disabled:opacity-50 pointer-coarse:h-11"
      >
        <option value="">Nothing picked</option>
        {options.map((item) => (
          <option key={item.id} value={item.id}>
            {item.title}
            {isClosed(item.state) ? ` (${STATE_LABELS[item.state].toLowerCase()})` : ""}
          </option>
        ))}
      </select>
    </label>
  );
}
