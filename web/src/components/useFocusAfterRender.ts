import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Returns a function that moves focus to the element `find` returns, in an effect after the next render: call it from
 * the handler that closes a dialog, sheet, menu or form, so the close and the request land in one render.
 *
 * React Aria keeps the page `inert` while an overlay is open, so focusing something behind it does nothing, and a
 * frame after the overlay unmounts it gives focus back to what opened it, but only if focus is on the body. An effect
 * after the commit that unmounted the overlay comes when the page is no longer inert and before that frame, so focus
 * is already somewhere real and is left alone. Never move focus on a timer or a guessed number of frames: where they
 * land against the commit and that frame depends on how slow the machine is.
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
