import { Check, Link2 } from "lucide-react";
import { useEffect, useState } from "react";

/**
 * The short link to a team, heap or item (design/readable-ids.md): `/h/` and its handles, which the router expands
 * to the full path. Below the app's base, so the local build's links work where it's served.
 */
export function shortLink(...handles: string[]): string {
  return new URL(`${import.meta.env.BASE_URL}h/${handles.map(encodeURIComponent).join("/")}`, window.location.origin)
    .href;
}

/** Copies the short link to what `handles` name, and says so for a moment. */
export function CopyLink({ handles, className = "" }: { handles: string[]; className?: string }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  return (
    <button
      type="button"
      onClick={() => void navigator.clipboard.writeText(shortLink(...handles)).then(() => setCopied(true))}
      className={`inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-ink-soft hover:bg-sunken ${className}`}
    >
      {copied ? <Check aria-hidden="true" className="size-3.5" /> : <Link2 aria-hidden="true" className="size-3.5" />}
      <span role="status">{copied ? "Copied" : "Copy link"}</span>
    </button>
  );
}
