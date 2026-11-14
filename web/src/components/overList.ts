import { useParams } from "@tanstack/react-router";
import type { MouseEvent } from "react";

import { rowOf } from "./ItemRow";

/**
 * For a link in the open item to another item (From, Blocked by, Became, an agent's mark in History). When the link
 * stays over the list on screen, the item open over a heap's list and the target in that heap, the list keeps its place
 * and the target's row is brought to the middle of the screen, as far as the list allows, where it shows selected.
 * Anywhere else the link leads to another page, which starts at the top. A target that isn't a row on screen (closed,
 * filtered out) scrolls nothing.
 */
export function useOverListLink() {
  const open = useParams({ strict: false });
  return (heap: { team: string; heap: string }, key: string) => {
    const overList = open.item !== undefined && open.team === heap.team && open.heap === heap.heap;
    return {
      resetScroll: !overList,
      onClick: (event: MouseEvent) => {
        // A new tab or window leaves this page as it is.
        if (!overList || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
          return;
        }
        rowOf(key)?.scrollIntoView({ block: "center" });
      },
    };
  };
}
