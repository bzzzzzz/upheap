import { Link } from "@tanstack/react-router";
import { motion } from "motion/react";
import type { KeyboardEvent } from "react";

import type { Item, RankedItem } from "../api/client";
import { SIZE_LABELS } from "./labels";
import { ParameterChips } from "./ParameterPicker";
import { TagChips } from "./TagChips";

/** What marks a row's link, for moving between rows with the arrow keys. */
const ROW_LINK = "data-row-link";

/** Focuses the row link `step` rows away from `from`, across the list's sections. */
function focusRow(from: HTMLElement, step: number) {
  const links = [...document.querySelectorAll<HTMLElement>(`[${ROW_LINK}]`)];
  links[links.indexOf(from) + step]?.focus();
}

/** Focuses the first row, for the arrow keys when nothing is focused yet. */
export function focusFirstRow() {
  document.querySelector<HTMLElement>(`[${ROW_LINK}]`)?.focus();
}

/** Focuses the row of `itemId`, for when its item closes. */
export function focusRowOf(itemId: string) {
  document.querySelector<HTMLElement>(`[${ROW_LINK}="${CSS.escape(itemId)}"]`)?.focus();
}

/**
 * One ranked item, for reading and ordering (app-ux.md, The list): the state circle, the position, the title, the due
 * chip and tags, and below them why it ranks where it does. The whole row opens the item; editing happens there.
 * `selected` marks the open item's row, and `itemOpen` makes opening another one replace it in the history, so Back
 * still closes the item instead of stepping through every row looked at.
 */
export function ItemRow({
  entry,
  heapId,
  selected,
  itemOpen,
  compact = false,
  onMarkDone,
}: {
  entry: RankedItem;
  heapId: string;
  selected: boolean;
  itemOpen: boolean;
  compact?: boolean;
  onMarkDone: (item: Item) => void;
}) {
  const { item, position, contributors } = entry;
  const canMarkDone = item.allowed_actions.includes("change_state");

  const onKeyDown = (event: KeyboardEvent<HTMLAnchorElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      focusRow(event.currentTarget, event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "x" && canMarkDone) {
      event.preventDefault();
      // The row goes away, so the next one takes the focus.
      focusRow(event.currentTarget, 1);
      onMarkDone(item);
    }
  };

  return (
    <motion.li
      layout="position"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.25 }}
      className={`relative flex items-center gap-3 rounded-lg px-2 py-2.5 has-[[data-row-link]:focus-visible]:outline-2 has-[[data-row-link]:focus-visible]:outline-accent pointer-coarse:py-3 ${
        selected ? "bg-accent-soft" : "hover:bg-sunken"
      }`}
    >
      <StateCircle item={item} disabled={!canMarkDone} onPress={() => onMarkDone(item)} />
      <span className="w-5 shrink-0 text-right text-sm text-muted tabular-nums" aria-label={`Position ${position}`}>
        {position}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          {/* The link covers the whole row, under the state circle. */}
          <Link
            to="/heaps/$heapId/items/$itemId"
            params={{ heapId, itemId: item.id }}
            search={true}
            replace={itemOpen}
            state={(prev) => ({ ...prev, closesBack: itemOpen ? prev.closesBack : true })}
            onKeyDown={onKeyDown}
            {...{ [ROW_LINK]: item.id }}
            aria-current={selected ? "true" : undefined}
            className="min-w-0 font-medium break-words outline-none after:absolute after:inset-0 after:content-['']"
          >
            {item.title}
          </Link>
          {item.due && <DueChip due={item.due} />}
          <TagChips tags={item.tags} />
          <ParameterChips parameters={item.parameters} className="hidden lg:inline-flex" />
        </div>
        {!compact && contributors.length > 0 && (
          <p className="mt-0.5 text-xs text-muted">{contributors.map((c) => c.label).join(" · ")}</p>
        )}
        {/* Under the contributors on a phone or tablet, where the first line has no room for them. */}
        <ParameterChips parameters={item.parameters} className="mt-1 flex lg:hidden" />
      </div>
      {(item.effort || item.impact) && (
        <span className="hidden shrink-0 text-xs text-ink-soft tabular-nums sm:inline">
          <span className="sr-only">Effort </span>
          {item.effort ? SIZE_LABELS[item.effort] : "–"}
          <span aria-hidden="true"> · </span>
          <span className="sr-only">, impact </span>
          {item.impact ? SIZE_LABELS[item.impact] : "–"}
        </span>
      )}
    </motion.li>
  );
}

/** A due time as the server labels it. Only an exact due time says "Due", and only it can be overdue. */
export function DueChip({ due }: { due: NonNullable<Item["due"]> }) {
  return (
    <span
      className={
        due.overdue
          ? "rounded-md bg-overdue-soft px-1.5 text-xs font-medium text-overdue-ink"
          : "rounded-md bg-sunken px-1.5 text-xs text-ink-soft"
      }
    >
      {due.overdue ? "Overdue · " : due.firm ? "Due " : ""}
      {due.label}
    </span>
  );
}

/**
 * The state at a glance, and the one-tap way to mark an item done: an empty circle for open, half filled for in
 * progress, dashed for blocked. Above the row's link, so a tap on it doesn't open the item.
 */
function StateCircle({ item, disabled, onPress }: { item: Item; disabled: boolean; onPress: () => void }) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onPress}
      aria-label={`Mark ${item.title} done`}
      title="Mark done"
      className="group relative z-10 -m-2 flex size-10 shrink-0 items-center justify-center rounded-full p-2 text-muted hover:text-done disabled:cursor-default disabled:opacity-50"
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" className="size-[22px]">
        {item.state === "in_progress" ? (
          <>
            <circle cx="12" cy="12" r="9" fill="none" stroke="var(--accent)" strokeWidth="2" />
            <path d="M12 3a9 9 0 0 1 0 18z" fill="var(--accent)" />
          </>
        ) : (
          <circle
            cx="12"
            cy="12"
            r="9"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeDasharray={item.state === "blocked" ? "3 3" : undefined}
          />
        )}
        <path
          d="m8 12.5 2.5 2.5 5.5-5.5"
          fill="none"
          stroke="var(--done)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="opacity-0 group-enabled:group-hover:opacity-100"
        />
      </svg>
    </button>
  );
}
