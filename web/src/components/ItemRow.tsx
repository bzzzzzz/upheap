import { Link } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useState, type KeyboardEvent } from "react";

import { ApiProblem, type ItemPatch, type ItemRef, type ItemState, type RankedItem } from "../api/client";
import { useUpdateItem } from "../api/queries";
import { BlockerSelect } from "./BlockerSelect";
import { STATE_LABELS, STATES } from "./labels";
import { ParameterChips } from "./ParameterSelect";
import { SizeSelect } from "./SizeSelect";
import { TagChips } from "./TagChips";

/** Messages for the errors an edit can bring back, by code. */
const PROBLEM_MESSAGES: Record<string, string> = {
  version_conflict: "Someone else changed this item. It now shows their version.",
  blocking_cycle: "That item already waits for this one, so it can't block it.",
  blocker_closed: "That item is closed, so nothing can wait for it.",
  blocker_not_in_heap: "That item isn't in this heap any more.",
  item_not_blocked: "Only a blocked item can wait for another one.",
};

/**
 * One ranked item. Controls appear only for actions the server allows. `blockerCandidates` are the heap's
 * non-closed items this one may wait for, offered as blockers when the item is blocked.
 */
export function ItemRow({
  entry,
  heapId,
  blockerCandidates,
  compact = false,
}: {
  entry: RankedItem;
  heapId: string;
  blockerCandidates: ItemRef[];
  compact?: boolean;
}) {
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
          <TagChips
            tags={item.tags}
            onRemove={
              canEdit
                ? (removed) => save({ tag_ids: item.tags.filter((t) => t.id !== removed.id).map((t) => t.id) })
                : undefined
            }
          />
          <ParameterChips parameters={item.parameters} />
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
            {item.state === "blocked" && (
              <BlockerSelect
                value={item.blocked_by}
                candidates={blockerCandidates}
                disabled={!canChangeState}
                onChange={(blockerId) => save({ blocked_by_item_id: blockerId })}
              />
            )}
          </div>
        )}
        {update.error && (
          <p role="alert" className="mt-1 text-xs text-red-700 dark:text-red-400">
            {(update.error instanceof ApiProblem && PROBLEM_MESSAGES[update.error.code]) || update.error.message}
          </p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-3 self-start">
        <Link
          to="/heaps/$heapId/items/$itemId"
          params={{ heapId, itemId: item.id }}
          aria-label={`Edit ${item.title}`}
          className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          Edit
        </Link>
        <label>
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
      </div>
    </motion.li>
  );
}
