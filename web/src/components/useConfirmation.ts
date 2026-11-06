import { useEffect, useRef, useState } from "react";

/**
 * A two-step action: the button asks, then Confirm or Cancel replace it. Cancel puts focus back on the button, which
 * the confirmation had replaced, so keyboard users don't land at the top of the page.
 */
export function useConfirmation() {
  const [confirming, setConfirming] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (!confirming && refocus.current) {
      refocus.current = false;
      buttonRef.current?.focus();
    }
  }, [confirming]);
  return {
    confirming,
    buttonRef,
    ask: () => setConfirming(true),
    cancel: () => {
      refocus.current = true;
      setConfirming(false);
    },
  };
}
