import type { ReactNode } from "react";

/** Whether the keyboard is a Mac's, which has ⌘ where the others have Ctrl. */
export const MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** Shown on a wide screen with a fine pointer only: the keys are on screen where there is room and a keyboard. */
export const WIDE_HINT = "hidden lg:pointer-fine:inline-flex";

/**
 * A key's name beside what it does. A hint is decoration, hidden from screen readers, which find the key where it
 * works; `readable` is for a list of keys, where the key is the content. `onInk` is for a hint on an ink-filled
 * button, such as the Add dialog's Add: it takes its colours from the button's own text instead of the surface's.
 * The two looks are separate class sets, because a colour passed in `className` doesn't reliably beat the default:
 * Tailwind's stylesheet order decides, not the order of the classes.
 */
export function Kbd({
  children,
  className = "",
  title,
  readable,
  onInk = false,
}: {
  children: ReactNode;
  className?: string;
  title?: string;
  readable?: boolean;
  onInk?: boolean;
}) {
  const look = onInk ? "border-canvas/30 text-canvas/70" : "border-line-strong bg-surface text-ink-soft";
  return (
    <kbd
      aria-hidden={readable ? undefined : "true"}
      title={title}
      className={`shrink-0 rounded-[5px] border px-1.5 text-[11px] font-normal ${look} ${className}`}
    >
      {children}
    </kbd>
  );
}
