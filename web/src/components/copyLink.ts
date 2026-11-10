import { useEffect, useState } from "react";

/**
 * The short link to a team, heap or item (design/architecture.md, Handles): `/h/` and its handles, which the router expands
 * to the full path. Below the app's base, so the local build's links work where it's served.
 */
export function shortLink(...handles: string[]): string {
  return new URL(`${import.meta.env.BASE_URL}h/${handles.map(encodeURIComponent).join("/")}`, window.location.origin)
    .href;
}

/** The link to share for an invite link's token: the API's `{origin}/invite/{token}`, below the app's base like `shortLink`. */
export function inviteLink(token: string): string {
  return new URL(`${import.meta.env.BASE_URL}invite/${encodeURIComponent(token)}`, window.location.origin).href;
}

/**
 * Copies text to the clipboard. `copy` answers whether the write worked, so a caller can offer the text to copy by hand
 * where it didn't: there is no clipboard on plain HTTP off localhost, and the browser may refuse. `copied` is true for
 * two seconds after a write works.
 */
export function useCopyText() {
  const [copiedAt, setCopiedAt] = useState<number | null>(null);
  useEffect(() => {
    if (copiedAt === null) return;
    const timer = window.setTimeout(() => setCopiedAt(null), 2000);
    return () => window.clearTimeout(timer);
  }, [copiedAt]);
  return {
    copied: copiedAt !== null,
    copy: async (text: string): Promise<boolean> => {
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        return false;
      }
      setCopiedAt(Date.now());
      return true;
    },
  };
}

/** Copies the short link to what the handles name; `copied` is true for a moment after. */
export function useCopyLink() {
  const { copied, copy } = useCopyText();
  return { copied, copy: (...handles: string[]) => void copy(shortLink(...handles)) };
}
