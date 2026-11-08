import { Link } from "@tanstack/react-router";
import { Ellipsis } from "lucide-react";
import { animate, motion, useMotionValue, type PanInfo } from "motion/react";
import { useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import type { HeapRef, Item, ItemState, RankedItem } from "../api/client";
import { changeLabel, isClosed, NEXT_STATES, SIZE_LABELS, STATE_LABELS } from "./labels";
import { TOUCH, useMedia } from "./media";
import { MenuEntry, MenuPopover } from "./Menu";
import { ParameterChips } from "./ParameterPicker";
import { TagChips } from "./TagChips";

/**
 * Changes an item's state from the list, and says so with an undo toast. `onError` runs if it fails, for a row to
 * come back from a swipe.
 */
export type ChangeState = (item: Item, state: ItemState, onError?: () => void) => void;

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
  document.querySelector<HTMLElement>(`[${ROW_LINK}="${CSS.escape(key)}"]`)?.focus();
}

/**
 * One ranked item, for reading and ordering (app-ux.md, The list): the state circle, the position, the title, the due
 * chip and tags, and below them why it ranks where it does. The whole row opens the item; editing happens there.
 * `selected` marks the open item's row, and `itemOpen` makes opening another one replace it in the history, so Back
 * still closes the item instead of stepping through every row looked at.
 *
 * With a finger, a row swipes (app-ux.md, The list §5): right marks it done, left shows two other states to pick.
 * With a mouse, hovering an open row shows Start and ⋯ for the other states (ux-overview.md, The list §1).
 */
export function ItemRow({
  entry,
  heapRef,
  selected,
  itemOpen,
  compact = false,
  onChangeState,
}: {
  entry: RankedItem;
  heapRef: HeapRef;
  selected: boolean;
  itemOpen: boolean;
  compact?: boolean;
  onChangeState: ChangeState;
}) {
  const { item, position, contributors } = entry;
  const canMarkDone = item.allowed_actions.includes("change_state");
  const onMarkDone = () => onChangeState(item, "done");
  const touch = useMedia(TOUCH);
  // On a touch screen a swipe does what the hover actions do.
  const actions = canMarkDone && !touch && !isClosed(item.state);
  const swipe = useSwipe(canMarkDone && touch, (state, onError) => onChangeState(item, state, onError));

  const onKeyDown = (event: KeyboardEvent<HTMLAnchorElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      focusRow(event.currentTarget, event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "x" && canMarkDone) {
      event.preventDefault();
      // The row goes away, so the next one takes the focus.
      focusRow(event.currentTarget, 1);
      onMarkDone();
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
        } ${actions ? (item.state === "open" ? "pr-22" : "pr-11") : ""}`}
      >
        <StateCircle item={item} disabled={!canMarkDone} onPress={onMarkDone} />
        <span className="w-5 shrink-0 text-right text-sm text-muted tabular-nums" aria-label={`Position ${position}`}>
          {position}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            {/* The link covers the whole row, under the state circle. */}
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
        {actions && <RowActions item={item} onChangeState={onChangeState} />}
      </motion.div>
    </motion.li>
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

/**
 * What a hovered row offers besides its circle: Start for an open item, and ⋯ for the other states. Over the row's
 * right edge, in room the row keeps for them, so they never cover the title or the sizes. Shown while the row is
 * hovered, has the focus (so the keyboard reaches them too) or has its menu open. ⋯ lists the states in the pane's
 * order (NEXT_STATES), less Done, which is the circle's, then Won't do.
 */
function RowActions({ item, onChangeState }: { item: Item; onChangeState: ChangeState }) {
  const start = item.state === "open";
  const next = isClosed(item.state) ? [] : NEXT_STATES[item.state as keyof typeof NEXT_STATES];
  const more = [...next, "wont_do" as const].filter(
    (state) => state !== "done" && !(start && state === "in_progress"),
  );
  return (
    <div className="absolute inset-y-0 right-0 z-10 flex items-center gap-0.5 bg-inherit pr-1.5 pl-2 opacity-0 group-hover/row:opacity-100 focus-within:opacity-100 has-[[aria-expanded=true]]:opacity-100">
      {start && (
        <button
          type="button"
          onClick={() => onChangeState(item, "in_progress")}
          aria-label={`Start ${item.title}`}
          className="h-8 rounded-lg px-2.5 text-xs font-medium text-ink-soft hover:bg-canvas hover:text-ink"
        >
          Start
        </button>
      )}
      <MenuTrigger>
        <Button
          aria-label={`More for ${item.title}`}
          className="flex size-8 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-canvas data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent"
        >
          <Ellipsis aria-hidden="true" className="size-4" />
        </Button>
        <MenuPopover placement="bottom end">
          <Menu
            aria-label={`More for ${item.title}`}
            className="outline-none"
            onAction={(key) => onChangeState(item, key as ItemState)}
          >
            {more.map((state) => (
              <MenuEntry key={state} id={state}>
                {changeLabel(item.state, state)}
              </MenuEntry>
            ))}
          </Menu>
        </MenuPopover>
      </MenuTrigger>
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
