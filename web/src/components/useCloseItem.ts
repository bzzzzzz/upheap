import { useNavigate, useRouter, useRouterState, type NavigateOptions } from "@tanstack/react-router";
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
      router.history.back();
    } else {
      // Closing by navigating says to the router itself that the list keeps its place.
      void navigate({ ...list, resetScroll: false });
    }
  }, [router, navigate, closesBack, list, itemKey, keepPlace]);
  return itemKey === undefined ? undefined : close;
}

/**
 * Keeps the list at `listPath` where it is when a Back leaves the item open over it. On its way back the router scrolls
 * to the top, and the browser to where the list was when the item opened, so the window goes back to its place once
 * both are done, before the next paint. The place is taken as the router starts the move, before either has scrolled.
 * The returned function keeps the place now, for a Back the app makes itself.
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
    const unsubscribeLoad = router.subscribe("onBeforeLoad", (event) => {
      const toList = event.toLocation.pathname === listPath && event.fromLocation?.pathname !== listPath;
      if (open.current && toList && keep.current === undefined && !window.matchMedia(PHONE).matches) {
        keep.current = windowPlace();
      }
    });
    const unsubscribeRendered = router.subscribe("onRendered", (event) => {
      const kept = keep.current;
      keep.current = undefined;
      if (kept && event.toLocation.pathname === listPath) {
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
