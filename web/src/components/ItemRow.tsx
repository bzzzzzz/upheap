import { Link } from "@tanstack/react-router";
import { Bot } from "lucide-react";
import { animate, motion, useMotionValue, type PanInfo } from "motion/react";
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";

import type { HeapRef, Item, ItemState, RankedItem, RankedList } from "../api/client";
import { KindIcon, StageIcon } from "./kindIcons";
import { STAGE_LABELS, STATE_LABELS } from "./labels";
import { TOUCH, useMedia } from "./media";
import { ParameterChips } from "./ParameterPicker";
import { formatDate } from "./reminders";
import { TagChips } from "./TagChips";

/**
 * Changes an item's state from the list, and says so with an undo toast. `onError` runs if it fails, for a row to
 * come back from a swipe.
 */
export type ChangeState = (item: Item, state: ItemState, onError?: () => void) => void;

/**
 * The list's tag and parameter filters as a row's chips use them: what is on, and how a chip turns its own value on
 * or off (ux.md, The list).
 */
export type RowFilters = {
  tags: ReadonlySet<string>;
  options: ReadonlySet<string>;
  toggleTag: (tagId: string) => void;
  toggleOption: (optionId: string) => void;
};

/** How far a row slides left to show its two actions, and how far right marks it done. */
const ACTIONS_WIDTH = 176;
const DONE_DISTANCE = 96;

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

/** Focuses the row of the item with `key`, for when its item closes. */
export function focusRowOf(key: string) {
  rowOf(key)?.focus();
}

/** The link of the row of the item with `key`, if it is in the list. */
export function rowOf(key: string) {
  return document.querySelector<HTMLElement>(`[${ROW_LINK}="${CSS.escape(key)}"]`);
}

/**
 * One ranked item, for reading and ordering (ux.md, The list): the main button, the title, the due chip and tags, and
 * below them why it ranks where it does. The whole row opens the item; editing happens there. A tag filters the list.
 * `selected` marks the open item's row, and `itemOpen` makes opening another one replace it in the history, so Back
 * still closes the item instead of stepping through every row looked at.
 *
 * The main button starts an open or blocked item and marks an in-progress one done. With a mouse, hovering or focusing
 * the play button also shows a done button beside it. With a finger, a row swipes (ux.md, The list): right marks it
 * done, left shows two other states to pick.
 */
export function ItemRow({
  entry,
  heapRef,
  selected,
  itemOpen,
  compact = false,
  filters,
  onChangeState,
}: {
  entry: RankedItem;
  heapRef: HeapRef;
  selected: boolean;
  itemOpen: boolean;
  compact?: boolean;
  filters: RowFilters;
  onChangeState: ChangeState;
}) {
  const { item, contributors } = entry;
  const canChangeState = item.allowed_actions.includes("change_state");
  const onMarkDone = () => onChangeState(item, "done");
  const onStart = () => onChangeState(item, "in_progress");
  // From the keyboard, the focus follows the row into In progress. A failed or cancelled start forgets it, so a later
  // start from elsewhere doesn't pull the focus into the list.
  const onStartFromKeyboard = () => {
    refocusAfterStart = item.key;
    onChangeState(item, "in_progress", () => (refocusAfterStart = undefined));
  };
  const startable = item.state === "open" || item.state === "blocked";
  const touch = useMedia(TOUCH);
  const swipe = useSwipe(canChangeState && touch, (state, onError) => onChangeState(item, state, onError));

  const onKeyDown = (event: KeyboardEvent<HTMLAnchorElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      focusRow(event.currentTarget, event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "x" && canChangeState) {
      event.preventDefault();
      // The row goes away, so the next one takes the focus.
      focusRow(event.currentTarget, 1);
      onMarkDone();
    } else if (event.key === "s" && canChangeState && startable) {
      event.preventDefault();
      onStartFromKeyboard();
    }
  };

  return (
    <motion.li
      layout="position"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.25 }}
      className="relative overflow-hidden rounded-lg has-[[data-row-link]:focus-visible]:outline-2 has-[[data-row-link]:focus-visible]:outline-accent"
    >
      {swipe.showActions && <SwipeActions item={item} open={swipe.open} onPick={swipe.pick} />}
      <motion.div
        {...swipe.dragProps}
        className={`group/row relative flex items-center gap-3 px-2 py-2.5 pointer-coarse:py-3 ${
          selected ? "bg-accent-soft" : "bg-canvas hover:bg-sunken"
        }`}
      >
        {/* Fixed slots, so titles line up whether or not the item can be changed. Done sits beside play with a
            mouse only: a touch screen swipes right instead. */}
        <span className="group/start flex shrink-0 items-center gap-3">
          <span className="size-6 shrink-0">
            {canChangeState &&
              (startable ? (
                <StartButton item={item} onPress={onStart} />
              ) : (
                item.state === "in_progress" && <DoneCircle item={item} onPress={onMarkDone} />
              ))}
          </span>
          {!touch && (
            <span className="relative z-10 w-5 shrink-0 opacity-0 group-focus-within/start:opacity-100 group-hover/start:opacity-100">
              {canChangeState && startable && <DoneButton item={item} onPress={onMarkDone} />}
            </span>
          )}
        </span>
        {/* A fixed slot, so titles line up whether or not the item is a task. */}
        <span className="flex w-3.5 shrink-0 justify-center">
          <KindIcon kind={item.kind} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            {/* Where the end of the row is tight, a dot says it; the words are in the link's name. */}
            {item.waits_for_you && (
              <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-accent sm:hidden" />
            )}
            {/* The same slot says an agent has the item, with a mark: the two never show together. */}
            {item.with_agent && <Bot aria-hidden="true" className="size-3.5 shrink-0 text-ink-soft sm:hidden" />}
            {/* The link covers the whole row, under the buttons and tags. */}
            <Link
              to="/teams/$team/heaps/$heap/items/$item"
              params={{ ...heapRef, item: item.key }}
              search={true}
              replace={itemOpen}
              state={(prev) => ({ ...prev, closesBack: itemOpen ? prev.closesBack : true })}
              onKeyDown={onKeyDown}
              // A swipe moves the row, not a dragged copy of the link.
              draggable={false}
              {...{ [ROW_LINK]: item.key }}
              aria-current={selected ? "true" : undefined}
              className="min-w-0 font-medium break-words outline-none after:absolute after:inset-0 after:content-['']"
            >
              {item.title}
              {/* The icons are for the eye; this is what a screen reader hears after the title. */}
              <span className="sr-only">
                {item.kind === "task" && ", task"}
                {item.kind === "reminder" && ", reminder"}
                {item.state === "blocked" && ", blocked"}
                {item.stage && item.stage !== "as_written" && `, ${STAGE_LABELS[item.stage].toLowerCase()}`}
                {item.step_progress && `, ${item.step_progress.ticked} of ${item.step_progress.total} steps ticked`}
                {item.waits_for_you && ", waits for you"}
                {item.with_agent && ", with an agent"}
              </span>
            </Link>
            <StageIcon stage={item.stage} />
            {item.step_progress && (
              <span aria-hidden="true" className="text-xs text-muted tabular-nums">
                {item.step_progress.ticked}/{item.step_progress.total}
              </span>
            )}
            {item.due && <DueChip due={item.due} />}
            <TagChips tags={item.tags} active={filters.tags} onToggle={filters.toggleTag} />
            <ParameterChips
              parameters={item.parameters}
              active={filters.options}
              onToggle={filters.toggleOption}
              className="hidden lg:inline-flex"
            />
          </div>
          {!compact && contributors.length > 0 && (
            <p className="mt-0.5 text-xs text-muted">{contributors.map((c) => c.label).join(" · ")}</p>
          )}
          {/* Under the contributors on a phone or tablet, where the first line has no room for them. */}
          <ParameterChips
            parameters={item.parameters}
            active={filters.options}
            onToggle={filters.toggleOption}
            className="mt-1 flex lg:hidden"
          />
        </div>
        {item.waits_for_you && (
          <span aria-hidden="true" className="hidden shrink-0 text-xs font-semibold text-accent-ink sm:inline">
            waits for you
          </span>
        )}
        {item.with_agent && (
          <span aria-hidden="true" className="hidden shrink-0 text-xs text-ink-soft sm:inline">
            with an agent
          </span>
        )}
      </motion.div>
    </motion.li>
  );
}

/**
 * A reminder still to come, in the group under the list (ux.md, The list §16): the reminder mark, the title, the date
 * it joins the list and its tags. No state button or swipe: it isn't in the ranking yet, and opening it is how its date
 * is changed.
 */
export function ComingRow({
  item,
  heapRef,
  selected,
  itemOpen,
  filters,
}: {
  item: Item;
  heapRef: HeapRef;
  selected: boolean;
  itemOpen: boolean;
  filters: RowFilters;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLAnchorElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      focusRow(event.currentTarget, event.key === "ArrowDown" ? 1 : -1);
    }
  };
  return (
    <li className="relative rounded-lg has-[[data-row-link]:focus-visible]:outline-2 has-[[data-row-link]:focus-visible]:outline-accent">
      <div
        className={`flex items-center gap-3 px-2 py-2.5 pointer-coarse:py-3 ${
          selected ? "bg-accent-soft" : "bg-canvas hover:bg-sunken"
        }`}
      >
        <span className="flex w-3.5 shrink-0 justify-center">
          <KindIcon kind={item.kind} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            {item.waits_for_you && (
              <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-accent sm:hidden" />
            )}
            <Link
              to="/teams/$team/heaps/$heap/items/$item"
              params={{ ...heapRef, item: item.key }}
              search={true}
              replace={itemOpen}
              state={(prev) => ({ ...prev, closesBack: itemOpen ? prev.closesBack : true })}
              onKeyDown={onKeyDown}
              {...{ [ROW_LINK]: item.key }}
              aria-current={selected ? "true" : undefined}
              className="min-w-0 font-medium break-words outline-none after:absolute after:inset-0 after:content-['']"
            >
              {item.title}
              <span className="sr-only">
                , reminder
                {item.waits_for_you && ", waits for you"}
              </span>
            </Link>
            {item.due && (
              <span className="rounded-md bg-sunken px-1.5 text-xs text-ink-soft">
                <span className="sr-only">Joins the list on </span>
                {formatDate(item.due.date)}
              </span>
            )}
            <TagChips tags={item.tags} active={filters.tags} onToggle={filters.toggleTag} />
          </div>
        </div>
        {item.waits_for_you && (
          <span aria-hidden="true" className="hidden shrink-0 text-xs font-semibold text-accent-ink sm:inline">
            waits for you
          </span>
        )}
      </div>
    </li>
  );
}

/**
 * The row's swipe: dragged right past `DONE_DISTANCE` it slides away and marks the item done; dragged left it stays
 * open on its actions until one is picked or the row is tapped. A tap right after a drag doesn't open the item.
 */
function useSwipe(enabled: boolean, change: (state: ItemState, onError: () => void) => void) {
  const x = useMotionValue(0);
  const [open, setOpen] = useState(false);
  // The actions are drawn only while the row moves or is open, so their colours never edge a row at rest.
  const [moving, setMoving] = useState(false);
  const dragged = useRef(false);
  const settle = (to: number) => animate(x, to, { type: "spring", bounce: 0, duration: 0.3 });
  const close = () => {
    setOpen(false);
    void settle(0).then(() => setMoving(false));
  };

  const onDragEnd = (event: globalThis.MouseEvent | TouchEvent | PointerEvent, info: PanInfo) => {
    window.setTimeout(() => (dragged.current = false));
    if (info.offset.x > DONE_DISTANCE) {
      const width = (event.target as HTMLElement | null)?.closest("li")?.offsetWidth ?? 400;
      void animate(x, width, { duration: 0.2 }).then(() => change("done", close));
    } else if (info.offset.x < -ACTIONS_WIDTH / 3 || (open && info.offset.x < 0)) {
      setOpen(true);
      void settle(-ACTIONS_WIDTH);
    } else {
      close();
    }
  };

  // Taps while open close the row instead of opening the item.
  const onClickCapture = (event: MouseEvent) => {
    if (dragged.current || open) {
      event.preventDefault();
      event.stopPropagation();
      if (open) close();
    }
  };

  return {
    enabled,
    open,
    showActions: enabled && (moving || open),
    pick: (state: ItemState) => {
      close();
      change(state, close);
    },
    dragProps: enabled
      ? {
          style: { x, touchAction: "pan-y" },
          drag: "x" as const,
          dragDirectionLock: true,
          dragConstraints: { left: -ACTIONS_WIDTH, right: 0 },
          dragElastic: { left: 0.1, right: 0.7 },
          dragMomentum: false,
          onDragStart: () => {
            dragged.current = true;
            setMoving(true);
          },
          onDragEnd,
          onClickCapture,
        }
      : {},
  };
}

/**
 * What a swipe shows under the row: Done on the left, for a swipe right; on the right, the two states besides open
 * and done, with Open in place of the one the item is in already.
 */
function SwipeActions({ item, open, onPick }: { item: Item; open: boolean; onPick: (state: ItemState) => void }) {
  const states: ItemState[] = [
    item.state === "in_progress" ? "open" : "in_progress",
    item.state === "blocked" ? "open" : "blocked",
  ];
  return (
    <div className="absolute inset-0 flex">
      <div aria-hidden="true" className="flex flex-1 items-center bg-done pl-5 text-sm font-semibold text-canvas">
        Done
      </div>
      {/* Out of the tab order and hidden from screen readers until the row is open; the item pane has them too. */}
      <div inert={!open} className="flex flex-1 justify-end">
        {states.map((state) => (
          <button
            key={state}
            type="button"
            onClick={() => onPick(state)}
            className={`w-[88px] text-sm font-semibold text-canvas ${state === "blocked" ? "bg-overdue-ink" : "bg-accent"}`}
          >
            {STATE_LABELS[state]}
          </button>
        ))}
      </div>
    </div>
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

const BUTTON =
  "group relative z-10 -m-2 flex size-10 shrink-0 items-center justify-center rounded-full p-2 text-muted pointer-coarse:-m-2.5 pointer-coarse:size-11 pointer-coarse:p-2.5";

/**
 * The one-tap way to start an open or blocked item: a circle with a play mark, dashed when the item is blocked, as the
 * row's one sign that it waits on something. Above the row's link, so a tap on it doesn't open the item.
 */
function StartButton({ item, onPress }: { item: Item; onPress: () => void }) {
  return (
    <button
      type="button"
      onClick={onPress}
      aria-label={`Start ${item.title}`}
      title="Start"
      className={`${BUTTON} hover:text-accent`}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" className="size-[22px]">
        <circle
          cx="12"
          cy="12"
          r="9"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeDasharray={item.state === "blocked" ? "3 3" : undefined}
        />
        <path
          d="M10 8.5v7l5.5-3.5z"
          fill="currentColor"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );
}

/** The one-tap way to mark an in-progress item done: a half-filled circle, with a check showing on hover. */
function DoneCircle({ item, onPress }: { item: Item; onPress: () => void }) {
  return (
    <button
      type="button"
      onClick={onPress}
      aria-label={`Mark ${item.title} done`}
      title="Mark done"
      className={`${BUTTON} hover:text-done`}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" className="size-[22px]">
        <circle cx="12" cy="12" r="9" fill="none" stroke="var(--accent)" strokeWidth="2" />
        <path d="M12 3a9 9 0 0 1 0 18z" fill="var(--accent)" />
        <path
          d="m8 12.5 2.5 2.5 5.5-5.5"
          fill="none"
          stroke="var(--done)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="opacity-0 group-hover:opacity-100"
        />
      </svg>
    </button>
  );
}

/**
 * A small circle with a check beside the play button, for marking an open or blocked item done with a mouse. Its
 * slot is hidden until the play button is hovered or focused, and it can be tabbed to either way.
 */
function DoneButton({ item, onPress }: { item: Item; onPress: () => void }) {
  return (
    <button
      type="button"
      onClick={onPress}
      aria-label={`Mark ${item.title} done`}
      title="Mark done"
      className="relative z-10 flex size-5 items-center justify-center rounded-full text-done hover:bg-done/15"
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" className="size-5">
        <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="2" />
        <path
          d="m8 12.5 2.5 2.5 5.5-5.5"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );
}

/** The key of the item `s` just started, so its row gets the focus back once it has moved (see below). */
let refocusAfterStart: string | undefined;

/**
 * Gives the focus back to the row `s` started. Starting moves the row to another section, so it remounts and the
 * focus falls to the page. Call it with the list, so it runs after the render that shows the item in progress.
 */
export function useRefocusStartedRow(list: RankedList | undefined) {
  useEffect(() => {
    const key = refocusAfterStart;
    if (key === undefined || !list) return;
    const entry = [...list.pinned, ...list.ranked, ...list.blocked].find((e) => e.item.key === key);
    if (entry?.item.state !== "in_progress") return;
    refocusAfterStart = undefined;
    // The row that had the focus may still be on screen for its exit animation: the new one takes over from it.
    const focused = document.activeElement;
    if (focused === document.body || focused?.getAttribute(ROW_LINK) === key) focusRowOf(key);
  }, [list]);
}
