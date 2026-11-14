import { useNavigate, useRouter, useRouterState, type NavigateOptions } from "@tanstack/react-router";
import { useCallback } from "react";

import { focusRowOf } from "./ItemRow";

/**
 * Closes the open item, `itemKey`, of the list at `list`: the heap's list, or the one across heaps. Back closes it
 * when the app opened it, so the history doesn't gain an entry per look at an item; opened from outside (a link, a
 * reload), it goes to the list instead. Focus goes back to the item's row. `list` should be stable between renders.
 */
export function useCloseItem(list: NavigateOptions, itemKey: string | undefined) {
  const router = useRouter();
  const navigate = useNavigate();
  const closesBack = useRouterState({ select: (state) => state.location.state.closesBack === true });
  const close = useCallback(() => {
    if (itemKey === undefined) return;
    // The list was under the item all along, so it stays where it is now. Closing by navigating says so to the router;
    // closing by Back can't, and on its way back the router scrolls to the top and the browser to where the list was
    // when the item opened. So the window goes back to its place once both are done, before the next paint.
    const { scrollX, scrollY } = window;
    const listPath = router.buildLocation(list).pathname;
    // Focus the row once the page it goes back to has rendered: before then, the row found may be another list's, on
    // its way out when the item was opened from another page, such as Waiting for me.
    const unsubscribe = router.subscribe("onRendered", (event) => {
      unsubscribe();
      if (closesBack && event.toLocation.pathname === listPath) {
        window.requestAnimationFrame(() => window.scrollTo(scrollX, scrollY));
      }
      window.setTimeout(() => focusRowOf(itemKey));
    });
    if (closesBack) {
      router.history.back();
    } else {
      void navigate({ ...list, resetScroll: false });
    }
  }, [router, navigate, closesBack, list, itemKey]);
  return itemKey === undefined ? undefined : close;
}
