import { useEffect, useState } from "react";

/**
 * The short link to a team, heap or item (design/readable-ids.md): `/h/` and its handles, which the router expands
 * to the full path. Below the app's base, so the local build's links work where it's served.
 */
export function shortLink(...handles: string[]): string {
  return new URL(`${import.meta.env.BASE_URL}h/${handles.map(encodeURIComponent).join("/")}`, window.location.origin)
    .href;
}

/** Copies the short link to what the handles name; `copied` is true for a moment after. */
export function useCopyLink() {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);
  return {
    copied,
    copy: (...handles: string[]) =>
      void navigator.clipboard.writeText(shortLink(...handles)).then(() => setCopied(true)),
  };
}
