import {
  useNavigate,
  useRouter,
  useRouterState,
  type NavigateOptions,
  type ParsedLocation,
} from "@tanstack/react-router";
import { useCallback, useEffect, useRef } from "react";

import { focusRowOf } from "./ItemRow";
import { PHONE } from "./media";

type Place = { x: number; y: number };

const windowPlace = (): Place => ({ x: window.scrollX, y: window.scrollY });

/**
 * Closes the open item, `itemKey`, of the list at `list`: the heap's list, or the one across heaps. Back closes it
 * when the app opened it, so the history doesn't gain an entry per look at an item; opened from outside (a link, a
 * reload), it goes to the list instead. Focus goes back to the item's row. `list` should be stable between renders.
 *
 * The list was under the item all along, so going Back to it, by ✕, Escape or the browser's own Back, leaves it where
 * it is now (see `useKeepPlaceOnBack`).
 */
export function useCloseItem(list: NavigateOptions, itemKey: string | undefined) {
  const router = useRouter();
  const navigate = useNavigate();
  const closesBack = useRouterState({ select: (state) => state.location.state.closesBack === true });
  const fullBack = useRouterState({ select: (state) => state.location.state.fullBack === true });
  const listPath = router.buildLocation(list).pathname;
  const keepPlace = useKeepPlaceOnBack(listPath, itemKey !== undefined);
  const close = useCallback(() => {
    if (itemKey === undefined) return;
    // Focus the row once the page it goes back to has rendered: before then, the row found may be another list's, on
    // its way out when the item was opened from another page, such as Waiting for me.
    const unsubscribe = router.subscribe("onRendered", () => {
      unsubscribe();
      window.setTimeout(() => focusRowOf(itemKey));
    });
    if (closesBack) {
      keepPlace();
      // From full screen entered from the pane, the pane is one entry back and the list another.
      if (fullBack) router.history.go(-2);
      else router.history.back();
    } else {
      // Closing by navigating says to the router itself that the list keeps its place.
      void navigate({ ...list, resetScroll: false });
    }
  }, [router, navigate, closesBack, fullBack, list, itemKey, keepPlace]);
  return itemKey === undefined ? undefined : close;
}

/**
 * Keeps the list at `listPath` where it is when a move leaves the item open over it for the list, or for another item
 * over the same list, as the browser's Back and Forward between two items do. The open item is in the path over a
 * heap's list, and in the search over All items. On a Back or Forward the router scrolls to the top, and the browser
 * to where the list was when that entry was left, so the window goes back to its place once both are done, before the
 * next paint. The place is taken as the router starts the move, before either has scrolled. The returned function
 * keeps the place now, for a Back the app makes itself.
 *
 * On a phone the item covers the list, and the browser's Back returns the list to where it was when the item opened.
 */
function useKeepPlaceOnBack(listPath: string, itemOpen: boolean) {
  const router = useRouter();
  // Read as the router starts a move, before the page under the item has rendered without it.
  const open = useRef(itemOpen);
  const keep = useRef<Place | undefined>(undefined);

  useEffect(() => {
    open.current = itemOpen;
  }, [itemOpen]);

  useEffect(() => {
    const itemPath = `${listPath}/items/`;
    const overList = (pathname: string) => pathname === listPath || pathname.startsWith(itemPath);
    // The item open at a location over the list: a heap's in the path, All items' in the search, as `item`. A view
    // change keeps it, so the list starts at the top as any view change does.
    const itemAt = (location: ParsedLocation) => {
      if (location.pathname.startsWith(itemPath)) return location.pathname;
      const item: unknown = location.pathname === listPath ? (location.search as { item?: unknown }).item : undefined;
      return typeof item === "string" ? item : undefined;
    };
    const unsubscribeLoad = router.subscribe("onBeforeLoad", (event) => {
      const from = event.fromLocation && itemAt(event.fromLocation);
      const stays = from !== undefined && overList(event.toLocation.pathname) && itemAt(event.toLocation) !== from;
      if (open.current && stays && keep.current === undefined && !window.matchMedia(PHONE).matches) {
        keep.current = windowPlace();
      }
    });
    const unsubscribeRendered = router.subscribe("onRendered", (event) => {
      const kept = keep.current;
      keep.current = undefined;
      if (kept && overList(event.toLocation.pathname)) {
        window.requestAnimationFrame(() => window.scrollTo(kept.x, kept.y));
      }
    });
    return () => {
      unsubscribeLoad();
      unsubscribeRendered();
    };
  }, [router, listPath]);

  return useCallback(() => {
    keep.current = windowPlace();
  }, []);
}
