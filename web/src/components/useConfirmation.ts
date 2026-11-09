import { useEffect, useRef, useState } from "react";

/**
 * Gives focus back to `ref`'s element once `away` turns false, when asked to with `returnFocus`: for a button that an
 * editor or a confirm replaced while `away` was true. It waits for the render that brings the button back, which a
 * frame's delay doesn't when the change comes from a request's answer.
 */
export function useReturnFocus<T extends HTMLElement>(away: boolean) {
  const ref = useRef<T>(null);
  const pending = useRef(false);
  useEffect(() => {
    if (!away && pending.current) {
      pending.current = false;
      ref.current?.focus();
    }
  }, [away]);
  return {
    ref,
    returnFocus: () => {
      pending.current = true;
    },
  };
}

/**
 * A two-step action: the button asks, then Confirm or Cancel replace it. Cancel puts focus back on the button, which
 * the confirmation had replaced, so keyboard users don't land at the top of the page.
 */
export function useConfirmation() {
  const [confirming, setConfirming] = useState(false);
  const { ref: buttonRef, returnFocus } = useReturnFocus<HTMLButtonElement>(confirming);
  return {
    confirming,
    buttonRef,
    ask: () => setConfirming(true),
    cancel: () => {
      returnFocus();
      setConfirming(false);
    },
    /** Closes without moving focus, for a confirm whose action no longer applies while focus is elsewhere. */
    dismiss: () => setConfirming(false),
  };
}
