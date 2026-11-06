import { motion } from "motion/react";
import { useState, type KeyboardEvent } from "react";

import { ApiProblem, type ItemPatch, type ItemState, type RankedItem } from "../api/client";
import { useUpdateItem } from "../api/queries";
import { STATE_LABELS, STATES } from "./labels";
import { SizeSelect } from "./SizeSelect";

/** One ranked item. Controls appear only for actions the server allows. */
export function ItemRow({ entry, heapId, compact = false }: { entry: RankedItem; heapId: string; compact?: boolean }) {
  const { item, position, contributors } = entry;
  const update = useUpdateItem(heapId);
  const [editingTitle, setEditingTitle] = useState(false);
  const canEdit = item.allowed_actions.includes("edit");
  const canChangeState = item.allowed_actions.includes("change_state");
  const save = (patch: ItemPatch) => update.mutate({ item, patch });

  const onTitleKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      setEditingTitle(false);
    } else if (event.key === "Enter") {
      const title = event.currentTarget.value;
      setEditingTitle(false);
      if (title.trim() !== "" && title !== item.title) save({ title });
    }
  };

  return (
    <motion.li
      layout="position"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.25 }}
      className="flex gap-3 border-b border-stone-200 py-3 last:border-b-0 dark:border-stone-800"
    >
      <span
        className="w-6 shrink-0 pt-0.5 text-right text-sm tabular-nums text-stone-400"
        aria-label={`Position ${position}`}
      >
        {position}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          {editingTitle ? (
            <input
              autoFocus
              defaultValue={item.title}
              onKeyDown={onTitleKey}
              onBlur={() => setEditingTitle(false)}
              aria-label="Title"
              className="min-w-0 flex-1 rounded border border-stone-300 bg-white px-1 dark:border-stone-700 dark:bg-stone-900"
            />
          ) : (
            <button
              type="button"
              disabled={!canEdit}
              onClick={() => setEditingTitle(true)}
              className="text-left font-medium disabled:cursor-default"
            >
              {item.title}
            </button>
          )}
          {item.due && (
            <span
              className={
                item.due.overdue
                  ? "rounded bg-red-100 px-1.5 text-xs font-medium text-red-800 dark:bg-red-950 dark:text-red-300"
                  : "rounded bg-stone-100 px-1.5 text-xs text-stone-600 dark:bg-stone-800 dark:text-stone-300"
              }
            >
              {item.due.overdue ? "Overdue · " : item.due.firm ? "Due " : ""}
              {item.due.label}
            </span>
          )}
        </div>
        {!compact && contributors.length > 0 && (
          <p className="mt-0.5 text-xs text-stone-500">{contributors.map((c) => c.label).join(" · ")}</p>
        )}
        {!compact && (
          <div className="mt-2 flex flex-wrap items-center gap-3">
            <SizeSelect
              label="Effort"
              value={item.effort}
              disabled={!canEdit}
              onChange={(effort) => save({ effort })}
            />
            <SizeSelect
              label="Impact"
              value={item.impact}
              disabled={!canEdit}
              onChange={(impact) => save({ impact })}
            />
            <label className="inline-flex items-center gap-1 text-xs text-stone-500">
              Due
              <input
                type="date"
                value={item.due?.date ?? ""}
                disabled={!canEdit}
                onChange={(e) =>
                  save({ due: e.target.value ? { date: e.target.value, firm: item.due?.firm ?? false } : null })
                }
                className="rounded border border-stone-300 bg-white px-1 py-0.5 text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
              />
            </label>
            {item.due && (
              <label className="inline-flex items-center gap-1 text-xs text-stone-500">
                <input
                  type="checkbox"
                  checked={item.due.firm}
                  disabled={!canEdit}
                  onChange={(e) => item.due && save({ due: { date: item.due.date, firm: e.target.checked } })}
                />
                Exact
              </label>
            )}
          </div>
        )}
        {update.error && (
          <p role="alert" className="mt-1 text-xs text-red-700 dark:text-red-400">
            {update.error instanceof ApiProblem && update.error.code === "version_conflict"
              ? "Someone else changed this item. It now shows their version."
              : update.error.message}
          </p>
        )}
      </div>
      <label className="shrink-0 self-start">
        <span className="sr-only">State</span>
        <select
          value={item.state}
          disabled={!canChangeState}
          onChange={(e) => save({ state: e.target.value as ItemState })}
          className="rounded border border-stone-300 bg-white px-1 py-0.5 text-sm disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900"
        >
          {STATES.map((state) => (
            <option key={state} value={state}>
              {STATE_LABELS[state]}
            </option>
          ))}
        </select>
      </label>
    </motion.li>
  );
}
