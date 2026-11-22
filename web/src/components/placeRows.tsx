import { useEffect, useRef } from "react";

import { requestIdOf, type HeapRef } from "../api/client";
import { usePlaceItem, useResetPlacement } from "../api/queries/placing";
import type { PlaceSide, RankedItem, RankedList } from "../api/schema";
import { itemProblem } from "./itemProblems";
import { rowOf } from "./ItemRow";
import { useToast } from "./UndoToast";
import { useDragToReorder, type GripProps } from "./useDragToReorder";

/**
 * Where the row `id` goes to land at `index` of the ranked group without it, as the server's request names it: below
 * the row that comes before the landing place, or above the first row when it lands first. `undefined` for a group of
 * one. Reading it at the index the row has now gives its place as it is, for putting it back.
 */
export function placementTarget(
  ranked: readonly RankedItem[],
  id: string,
  index: number,
): { anchor: string; side: PlaceSide } | undefined {
  const others = ranked.filter((entry) => entry.item.id !== id);
  const before = others[index - 1];
  if (before) return { anchor: before.item.id, side: "below" };
  const first = others[0];
  return first && { anchor: first.item.id, side: "above" };
}

/** What a ranked row needs to be placed by hand: the drag's handlers, how far it is lifted, and the moves it can make. */
export type RowPlace = {
  rowRef: (row: HTMLLIElement | null) => void;
  /** A wide screen's grip. */
  grip: GripProps;
  /** A long press, on a touch screen. */
  press: GripProps;
  /** How far the row is dragged, while it is. */
  lift: number | null;
  /** Whether the latest press on the grip went on to drag, so that its end doesn't count as a click. */
  dragged: () => boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  /** Moves the row one place; `byKey` when the keyboard did, so the focus stays on the row. */
  onMove: (delta: -1 | 1, byKey: boolean) => void;
};

/**
 * Placing the rows of the list's ranked group by hand (ux.md, The list): by the grip, a long press, the keys or the
 * grip's menu. A move asks the server to put the item next to the row above its landing place, and says so with a toast
 * that offers Undo. Undo takes the placing away if the item had none, and puts it next to the row it had above it
 * (below it, for the first row) if it had one. A refused move shows the problem, and the list comes back from the
 * server.
 */
export function usePlaceRows(heapRef: HeapRef, list: Pick<RankedList, "ranked" | "as_of">) {
  const place = usePlaceItem(heapRef);
  const reset = useResetPlacement(heapRef);
  const setToast = useToast();
  const listRef = useRef<HTMLOListElement>(null);
  const rows = useRef(new Map<string, HTMLElement>());
  const { ranked, as_of: asOf } = list;
  const order = ranked.map((entry) => entry.item.id).join(",");
  // The item whose row keeps the focus once it has moved: moving the node can drop the focus to the page.
  const refocus = useRef<string | undefined>(undefined);
  useEffect(() => {
    const key = refocus.current;
    if (key === undefined) return;
    refocus.current = undefined;
    if (document.activeElement === document.body) rowOf(key)?.focus();
  }, [order]);

  const failed = (error: Error) =>
    setToast({ message: itemProblem(error), error: true, requestId: requestIdOf(error) });

  // Puts the item at `index` of the group without it. A move to where it is asks for nothing.
  const move = (id: string, index: number) => {
    const from = ranked.findIndex((entry) => entry.item.id === id);
    const entry = ranked[from];
    const target = placementTarget(ranked, id, index);
    if (!entry || !target || from === index) return;
    const { item } = entry;
    const back = placementTarget(ranked, id, from);
    place.mutateAsync({ item, ...target, asOf }).then((saved) => {
      setToast({
        message: `Moved “${item.title}” ${index < from ? "up" : "down"}.`,
        undo: () => {
          const undone =
            item.placement === null || !back
              ? reset.mutateAsync({ item: saved })
              : place.mutateAsync({ item: saved, ...back, asOf });
          undone.then(undefined, failed);
        },
      });
    }, failed);
  };

  const drag = useDragToReorder({
    listRef,
    rows,
    ids: ranked.map((entry) => entry.item.id),
    onDrop: move,
  });

  return {
    listRef,
    lineY: drag.lineY,
    active: drag.active,
    rowPlace: (id: string, index: number): RowPlace => ({
      rowRef: (row) => {
        if (row) rows.current.set(id, row);
        else rows.current.delete(id);
      },
      grip: drag.gripProps(id),
      press: drag.pressProps(id),
      lift: drag.lifted === id ? drag.dy : null,
      dragged: drag.dragged,
      canMoveUp: index > 0,
      canMoveDown: index < ranked.length - 1,
      onMove: (delta, byKey) => {
        if (byKey) refocus.current = ranked[index]?.item.key;
        move(id, index + delta);
      },
    }),
  };
}
