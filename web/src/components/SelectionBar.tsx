import { useNavigate } from "@tanstack/react-router";
import {
  ArrowRightLeft,
  Bot,
  Check,
  CircleCheck,
  CirclePause,
  CirclePlay,
  CircleX,
  Minus,
  X,
  type LucideIcon,
} from "lucide-react";
import { useRef, useState } from "react";

import { ApiProblem, type HeapRef } from "../api/client";
import type { Item } from "../api/schema";
import { useMe } from "../api/queries/me";
import { useMoveItems } from "../api/queries/moves";
import { plural, RefusedPanel, useBulkActions, type BulkAction, type Refused } from "./bulkActions";
import { buttonClass, compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { MoveToHeap } from "./MoveToHeap";
import { PopoverSheet } from "./PopoverSheet";
import { Saving } from "./Saving";
import { useToast } from "./UndoToast";

/**
 * The bars of select mode on a heap's list (ux.md, The list §19), the bulk actions on them, and the Move to… picker. On
 * a tablet or wide screen one bar replaces the toolbar: a box, how many are selected, Select all, the actions and ✕.
 * On a phone a bar over the top bar has ✕ and Select all, and one over the tabs has the actions. `items` are the rows
 * that can be selected, as the list shows them; `selected` holds their IDs. `onSelect` takes the whole new selection,
 * `onDone` ends select mode.
 */
export function SelectionBars({
  heapRef,
  items,
  selected,
  onSelect,
  onDone,
}: {
  heapRef: HeapRef;
  items: Item[];
  selected: ReadonlySet<string>;
  onSelect: (ids: Set<string>) => void;
  onDone: () => void;
}) {
  const { data: me } = useMe();
  const team = me?.teams.find((t) => t.slug === heapRef.team);
  const move = useMoveItems(heapRef);
  const navigate = useNavigate();
  const toast = useToast();
  const moveRef = useRef<HTMLButtonElement>(null);
  const [moving, setMoving] = useState(false);
  const [refused, setRefused] = useState<Refused | null>(null);

  const count = selected.size;
  const all = items.length > 0 && count === items.length;
  const selectAll = () => onSelect(new Set(items.map((item) => item.id)));
  const toggleAll = () => (all ? onSelect(new Set()) : selectAll());
  const chosen = items.filter((item) => selected.has(item.id));

  const bulk = useBulkActions({ heapRef, chosen, onSelect, onDone });
  const busy = bulk.running !== null;
  const open = () => {
    move.reset();
    setMoving(true);
  };
  const close = () => {
    setMoving(false);
    setRefused(null);
  };
  const moveTo = async (to: HeapRef & { id: string }, name: string) => {
    try {
      const answer = await move.mutateAsync({ items: chosen, to });
      if (answer.not_moved.length === 0) {
        const rekeyed = answer.moved.filter((moved) => chosen.find((item) => item.id === moved.id)?.key !== moved.key);
        const keys =
          rekeyed.length === 0 ? "" : ` ${rekeyed.length} got ${rekeyed.length === 1 ? "a new key" : "new keys"}.`;
        toast({
          message: `Moved ${plural(answer.moved.length)} to ${name}.${keys}`,
          action: {
            label: `Open ${name}`,
            run: () => void navigate({ to: "/teams/$team/heaps/$heap", params: { team: to.team, heap: to.heap } }),
          },
        });
        close();
        onDone();
      } else {
        setRefused({
          headline:
            answer.moved.length === 0
              ? `None of the ${plural(chosen.length)} moved.`
              : `Moved ${answer.moved.length} of ${plural(chosen.length)} to ${name}.`,
          heading: "Not moved:",
          entries: answer.not_moved.map(({ id, problem }) => ({
            id,
            title: chosen.find((item) => item.id === id)?.title ?? "An item",
            reason: itemProblem(new ApiProblem(problem.status, problem.code, problem.title)),
          })),
        });
      }
    } catch {
      // The mutation holds the error, and the picker shows it.
    }
  };
  // Close keeps select mode, with only the items that stayed selected; those no longer shown are dropped with the rest.
  const closeRefused = () => {
    if (refused) onSelect(new Set(refused.entries.map((entry) => entry.id)));
    close();
  };

  const status = (
    <span role="status" className="text-sm font-medium tabular-nums">
      {count} selected
    </span>
  );
  const startAction: BulkAction = bulk.pausing ? "pause" : "start";
  const startLabel = bulk.pausing ? "Pause" : "Start";
  const actionButton = (
    action: BulkAction,
    label: string,
    className: string,
    shortLabel?: string,
    Icon?: LucideIcon,
  ) => (
    <button
      type="button"
      aria-label={shortLabel ? label : undefined}
      aria-busy={bulk.running === action || undefined}
      disabled={count === 0 || busy}
      onClick={(event) => bulk.begin(action, event.currentTarget)}
      className={className}
    >
      {Icon && bulk.running !== action && <Icon aria-hidden="true" className="size-5" />}
      <Saving pending={bulk.running === action} className={Icon ? "size-5 items-center justify-center" : ""} />
      {shortLabel ?? label}
    </button>
  );
  const wideClass = compactButtonClass.bordered;
  const phoneClass = `${buttonClass} min-h-12 flex-1 flex-col gap-0.5 rounded-lg px-1 text-xs text-ink`;

  return (
    <>
      <div className="mt-6 flex flex-wrap items-center gap-3 max-sm:hidden">
        <button
          type="button"
          role="checkbox"
          aria-checked={all ? true : count > 0 ? "mixed" : false}
          aria-label="Select all"
          onClick={toggleAll}
          className={`flex size-6 shrink-0 items-center justify-center rounded-md border-2 ${
            count > 0 ? "border-accent bg-accent text-canvas" : "border-line-strong bg-surface"
          }`}
        >
          {all ? (
            <Check aria-hidden="true" className="size-4" strokeWidth={3} />
          ) : (
            count > 0 && <Minus aria-hidden="true" className="size-4" strokeWidth={3} />
          )}
        </button>
        {status}
        <span aria-hidden="true" className="text-muted">
          ·
        </span>
        <button type="button" onClick={selectAll} className={compactButtonClass.quiet}>
          Select all {items.length}
        </button>
        <span className="ml-auto flex flex-wrap items-center gap-2">
          {actionButton(startAction, startLabel, wideClass)}
          {actionButton("done", "Done", wideClass)}
          {actionButton("wont_do", "Won't do", wideClass)}
          {actionButton("send", "Send to an agent…", wideClass)}
          <button ref={moveRef} type="button" disabled={count === 0 || busy} onClick={open} className={wideClass}>
            Move to…
          </button>
          <button type="button" aria-label="Leave select mode" onClick={onDone} className={compactButtonClass.icon}>
            <X aria-hidden="true" className="size-5" />
          </button>
        </span>
      </div>

      <div className="fixed inset-x-0 top-0 z-20 flex items-center gap-1 bg-surface px-2 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1 sm:hidden">
        <button type="button" aria-label="Leave select mode" onClick={onDone} className={compactButtonClass.icon}>
          <X aria-hidden="true" className="size-5" />
        </button>
        {status}
        <button type="button" onClick={toggleAll} className={`${compactButtonClass.quiet} ml-auto`}>
          {all ? "Clear all" : "Select all"}
        </button>
      </div>
      <div className="fixed inset-x-0 bottom-0 z-20 border-t border-line bg-surface px-4 pt-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] sm:hidden">
        <div className="flex gap-1">
          {actionButton("done", "Done", phoneClass, "Done", CircleCheck)}
          {actionButton("wont_do", "Won't do", phoneClass, "Won't do", CircleX)}
          {actionButton(startAction, startLabel, phoneClass, startLabel, bulk.pausing ? CirclePause : CirclePlay)}
          {actionButton("send", "Send to an agent…", phoneClass, "Agent…", Bot)}
          <button
            type="button"
            aria-label="Move to…"
            disabled={count === 0 || busy}
            onClick={open}
            className={phoneClass}
          >
            <ArrowRightLeft aria-hidden="true" className="size-5" />
            Move…
          </button>
        </div>
      </div>
      {bulk.overlays}

      {team && me && (
        <PopoverSheet
          title="Move to"
          triggerRef={moveRef}
          isOpen={moving}
          onClose={() => !move.isPending && (refused ? closeRefused() : close())}
          placement="bottom end"
          size="narrow"
        >
          {refused ? (
            <RefusedPanel refused={refused} onClose={closeRefused} />
          ) : (
            <MoveToHeap
              me={me}
              teams={[team]}
              heapRef={heapRef}
              pending={move.isPending}
              error={move.error}
              count={count}
              onMove={(to, name) => void moveTo(to, name)}
              onReset={move.reset}
            />
          )}
        </PopoverSheet>
      )}
    </>
  );
}
