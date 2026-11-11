import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Returns a function that moves focus to the element `find` returns, in an effect after the next render: call it from
 * the handler that closes a dialog, sheet, menu or form, so the close and the request land in one render.
 *
 * While an overlay is open, React Aria keeps the rest of the page `inert`, so focusing something behind it does
 * nothing. When it closes, the overlay first stays mounted for a render while it exits, with focus containment already
 * off, and its `inert` is lifted in a passive-effect cleanup; React runs every passive cleanup before any passive
 * effect, so this hook's effect finds the page live. A frame after the overlay unmounts, React Aria gives focus back to
 * what opened it, but only if focus is on the body, so by then it leaves this focus alone. It must stay a passive
 * effect: a layout effect or a callback ref runs while the page is still inert. Never move focus on a timer or a
 * guessed number of frames: where they land against these steps depends on how slow the machine is.
 *
 * For a button an editor or a confirm replaced, which comes back some renders later, use `useReturnFocus`
 * (useConfirmation.ts) instead.
 *
 * `find` runs after the render, so it can look up an element that only exists then. If it finds none, nothing happens.
 */
export function useFocusAfterRender() {
  const target = useRef<(() => HTMLElement | null | undefined) | null>(null);
  const [request, setRequest] = useState(0);
  useEffect(() => {
    const find = target.current;
    if (find === null) return;
    target.current = null;
    find()?.focus();
  }, [request]);
  return useCallback((find: () => HTMLElement | null | undefined) => {
    target.current = find;
    setRequest((n) => n + 1);
  }, []);
}
