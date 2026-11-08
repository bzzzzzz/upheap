// Whether the Add dialog stays open for the next item (ux-overview.md, Adding §3): remembered per device, in this
// browser's storage, so it's as the user left it whichever way they open the dialog, also after a reload.
import { useState } from "react";

const KEY = "upheap.add-another";

/** Whether Add another is ticked, off until the user ticks it, and the way to change it. */
export function useAddAnother(): [boolean, (addAnother: boolean) => void] {
  const [addAnother, setAddAnother] = useState(() => {
    try {
      return localStorage.getItem(KEY) === "true";
    } catch {
      // Storage may be blocked or unavailable; it's off then.
      return false;
    }
  });
  const change = (next: boolean) => {
    setAddAnother(next);
    try {
      localStorage.setItem(KEY, String(next));
    } catch {
      // Not remembered, but it still holds while the dialog is open.
    }
  };
  return [addAnother, change];
}
