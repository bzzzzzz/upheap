// Whether the person folded Your decision on the Agents page (design/ux.md, Agents). It is about this device's habits,
// so it lives in localStorage like the heap switcher's folded teams. Storage can be blocked or hold anything, so a
// failed read is "not folded" and a failed write just isn't remembered.

const DECISIONS_FOLDED_KEY = "upheap.decisionsFolded";

export function readDecisionsFolded(): boolean {
  try {
    return window.localStorage.getItem(DECISIONS_FOLDED_KEY) === "true";
  } catch {
    return false;
  }
}

export function writeDecisionsFolded(folded: boolean) {
  try {
    window.localStorage.setItem(DECISIONS_FOLDED_KEY, String(folded));
  } catch {
    // Not remembered.
  }
}
