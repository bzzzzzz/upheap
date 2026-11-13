import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { Button } from "react-aria-components";

import { ApiProblem, type BlockerGone, type HeapRef, type WaitingBlocker } from "../api/client";
import { useUnblock } from "../api/queries";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";

const focusClass = "outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent";

const isChangedMeanwhile = (error: Error) => error instanceof ApiProblem && error.code === "version_conflict";

/** What happened to the blocker: "closed as won't do", "closed as done", "which became other items" or "moved to <heap>". */
function goneWords(gone: BlockerGone) {
  const heap = gone.moved_to ? gone.moved_to.name : null;
  switch (gone.how) {
    case "done":
      return heap ? `closed as done in ${heap}` : "closed as done";
    case "wont_do":
      return heap ? `closed as won't do in ${heap}` : "closed as won't do";
    case "became":
      return heap ? `became other items in ${heap}` : "which became other items";
    case "moved":
      return heap ? `moved to ${heap}` : "moved to another heap";
    default:
      return "gone";
  }
}

/** The head word of a gone-blocker row: "Blocker moved" for a move, "Blocker closed" otherwise. */
export function blockerGoneWord(gone: BlockerGone) {
  return gone.how === "moved" ? "Blocker moved" : "Blocker closed";
}

/**
 * A blocked item whose blocker was closed as won't do, moved to another heap, or closed as done without freeing it:
 * it stays blocked and waits for the person to move it on (ux.md, Waiting for me). Shown on the Waiting for me page and in the item's Waits for you,
 * with "Waits for “<blocker>”, closed as won't do", the blocker's title opening it, and Unblock, which moves the item
 * to open at once. `open`, on the page, adds Open, which opens the item to close it or pick another blocker; in the
 * item it would open what is already open. `line` is the row's own first line.
 */
export function BlockerGoneRow({
  heapRef,
  item,
  blocker,
  line,
  open = false,
  onUnblocked,
  onElsewhere,
}: {
  heapRef: HeapRef;
  item: { id: string; key: string; title: string; version: number };
  blocker: WaitingBlocker;
  line: ReactNode;
  open?: boolean;
  onUnblocked: () => void;
  /** The item was changed by someone else first. */
  onElsewhere: () => void;
}) {
  const unblock = useUnblock(heapRef);
  const canUnblock = blocker.allowed_actions.includes("change_state");
  const unblockNow = () =>
    unblock.mutateAsync(item).then(onUnblocked, (error: Error) => {
      if (isChangedMeanwhile(error)) onElsewhere();
    });
  const error = unblock.error && !isChangedMeanwhile(unblock.error) ? unblock.error : null;

  return (
    <li data-item={item.id} data-blocker-gone="" className="flex flex-col gap-1.5 py-3 first:pt-0 last:pb-0">
      {line}
      <p className="text-sm break-words">
        Waits for “
        <Link
          to="/teams/$team/heaps/$heap/items/$item"
          params={{ ...heapRef, heap: blocker.gone.moved_to?.slug ?? heapRef.heap, item: blocker.item.key }}
          className="text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent"
        >
          {blocker.item.title}
        </Link>
        ”, {goneWords(blocker.gone)}
      </p>
      {(canUnblock || open) && (
        <div className="flex gap-2 max-sm:grid max-sm:grid-cols-2">
          {canUnblock && (
            <Button
              isDisabled={unblock.isPending}
              onPress={() => {
                unblock.reset();
                void unblockNow();
              }}
              aria-label={`Unblock ${item.title}`}
              className={`${compactButtonClass.primary} ${focusClass}`}
            >
              Unblock
            </Button>
          )}
          {open && (
            <Link
              to="/teams/$team/heaps/$heap/items/$item"
              params={{ ...heapRef, item: item.key }}
              // Closing the item goes back here, not to the heap's list.
              state={(prev) => ({ ...prev, closesBack: true })}
              aria-label={`Open ${item.title}`}
              className={`${compactButtonClass.bordered} inline-flex items-center justify-center outline-none focus-visible:outline-2 focus-visible:outline-accent`}
            >
              Open
            </Link>
          )}
        </div>
      )}
      {error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(error)}
        </p>
      )}
    </li>
  );
}
