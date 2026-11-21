import type { ReactNode } from "react";

/** Whether the keyboard is a Mac's, which has ⌘ where the others have Ctrl. */
export const MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** Shown on a wide screen with a fine pointer only: the keys are on screen where there is room and a keyboard. */
export const WIDE_HINT = "hidden lg:pointer-fine:inline-flex";

/**
 * A key's name beside what it does. A hint is decoration, hidden from screen readers, which find the key where it
 * works; `readable` is for a list of keys, where the key is the content.
 */
export function Kbd({
  children,
  className = "",
  title,
  readable,
}: {
  children: ReactNode;
  className?: string;
  title?: string;
  readable?: boolean;
}) {
  return (
    <kbd
      aria-hidden={readable ? undefined : "true"}
      title={title}
      className={`shrink-0 rounded-[5px] border border-line-strong bg-surface px-1.5 text-[11px] font-normal text-ink-soft ${className}`}
    >
      {children}
    </kbd>
  );
}
