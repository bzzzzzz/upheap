import { useNavigate } from "@tanstack/react-router";
import { Check, Minus, X } from "lucide-react";
import { useRef, useState } from "react";

import { ApiProblem, type HeapRef } from "../api/client";
import type { Item } from "../api/schema";
import { useMe } from "../api/queries/me";
import { useMoveItems } from "../api/queries/moves";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { MoveToHeap } from "./MoveToHeap";
import { PopoverSheet } from "./PopoverSheet";
import { useToast } from "./UndoToast";

/** What the batch move said when some items stayed: where they went and why each didn't. */
type Refused = {
  target: string;
  moved: number;
  total: number;
  notMoved: { id: string; title: string; reason: string }[];
};

const plural = (count: number) => `${count} item${count === 1 ? "" : "s"}`;

/**
 * The bars of select mode on a heap's list (ux.md, The list §19), and the Move to… picker they open. On a tablet or
 * wide screen one bar replaces the toolbar: a box, how many are selected, Select all, Move to… and Done. On a phone a
 * bar over the top bar has ✕ and Select all, and one over the tabs has Move to…. `items` are the rows that can be
 * selected, as the list shows them; `selected` holds their IDs. `onSelect` takes the whole new selection, `onDone` ends
 * select mode.
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
          target: name,
          moved: answer.moved.length,
          total: chosen.length,
          notMoved: answer.not_moved.map(({ id, problem }) => ({
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
    if (refused) onSelect(new Set(refused.notMoved.map((entry) => entry.id)));
    close();
  };

  const status = (
    <span role="status" className="text-sm font-medium tabular-nums">
      {count} selected
    </span>
  );
  const moveButton = (className: string, ref?: typeof moveRef) => (
    <button ref={ref} type="button" disabled={count === 0} onClick={open} className={className}>
      Move to…
    </button>
  );

  return (
    <>
      <div className="mt-6 flex items-center gap-3 max-sm:hidden">
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
        <span className="ml-auto flex items-center gap-2">
          {moveButton(compactButtonClass.bordered, moveRef)}
          <button type="button" onClick={onDone} className={compactButtonClass.primary}>
            Done
          </button>
        </span>
      </div>

      <div className="fixed inset-x-0 top-0 z-20 flex items-center gap-1 bg-surface px-2 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1 sm:hidden">
        <button type="button" aria-label="Done selecting" onClick={onDone} className={compactButtonClass.icon}>
          <X aria-hidden="true" className="size-5" />
        </button>
        {status}
        <button type="button" onClick={toggleAll} className={`${compactButtonClass.quiet} ml-auto`}>
          {all ? "Clear all" : "Select all"}
        </button>
      </div>
      <div className="fixed inset-x-0 bottom-0 z-20 border-t border-line bg-surface px-4 pt-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] sm:hidden">
        {moveButton(`${compactButtonClass.primary} h-12 w-full`)}
      </div>

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
            <MoveRefused refused={refused} onClose={closeRefused} />
          ) : (
            <MoveToHeap
              me={me}
              team={team}
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

/** What a batch move left behind: how many moved, and each item that didn't, with the reason. */
function MoveRefused({ refused, onClose }: { refused: Refused; onClose: () => void }) {
  return (
    <div className="flex flex-col gap-3">
      <h3 className="text-base font-semibold break-words">
        {refused.moved === 0
          ? `None of the ${plural(refused.total)} moved.`
          : `Moved ${refused.moved} of ${plural(refused.total)} to ${refused.target}.`}
      </h3>
      <p className="text-sm font-medium">Not moved:</p>
      <ul className="flex flex-col gap-1 text-sm text-ink-soft">
        {refused.notMoved.map((entry) => (
          <li key={entry.id} className="break-words">
            {entry.title} — {entry.reason}
          </li>
        ))}
      </ul>
      <div className="flex justify-end">
        <button type="button" autoFocus onClick={onClose} className={compactButtonClass.primary}>
          Close
        </button>
      </div>
    </div>
  );
}
