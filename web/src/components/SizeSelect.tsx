import type { Size } from "../api/client";
import { SIZE_LABELS, SIZES } from "./labels";

/** Effort or impact picker. The empty option means unset. */
export function SizeSelect({
  label,
  value,
  onChange,
  disabled,
}: {
  label: string;
  value: Size | null | undefined;
  onChange: (value: Size | null) => void;
  disabled?: boolean;
}) {
  return (
    <label className="inline-flex items-center gap-1 text-xs text-stone-500">
      <span>{label}</span>
      <select
        value={value ?? ""}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value === "" ? null : (e.target.value as Size))}
        className="rounded border border-stone-300 bg-white px-1 py-0.5 text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
      >
        <option value="">–</option>
        {SIZES.map((size) => (
          <option key={size} value={size}>
            {SIZE_LABELS[size]}
          </option>
        ))}
      </select>
    </label>
  );
}
