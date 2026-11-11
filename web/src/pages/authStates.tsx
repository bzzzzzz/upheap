// The pieces the pages a link opens share (ux.md, Invite page and Reset page): outside the app's shell, in the sign-in
// card, each state a heading, a line or two and the way back.
import { Link } from "@tanstack/react-router";
import { useEffect, useRef, type ReactNode, type RefObject } from "react";

import { problemMessage } from "../components/ProblemText";
import { authButtonClass } from "./AuthPage";

/** Whether the page has shown a state already, so a heading that mounts now replaces one and takes focus. */
export type MoveFocus = RefObject<boolean>;

/**
 * A `MoveFocus` for a page that reads its link: off while `loading`, on once the first state shows. Focus moves to
 * the heading when the page changes state, not when it first loads. A child's effect runs before this one, so a
 * heading that mounts with the first state sees it still off.
 */
export function useMoveFocus(loading: boolean): MoveFocus {
  const moveFocus = useRef(false);
  useEffect(() => {
    if (!loading) moveFocus.current = true;
  }, [loading]);
  return moveFocus;
}

/** A read that failed: the problem where the content would be, and the way to try again. */
export function Retry({ error, onRetry }: { error: Error | null; onRetry: () => void }) {
  return (
    <div className="flex flex-col gap-4">
      <p role="alert" className="text-sm font-medium text-overdue-ink">
        {error ? problemMessage(error) : "The link couldn't be read."}
      </p>
      <button type="button" onClick={onRetry} className={authButtonClass}>
        Try again
      </button>
    </div>
  );
}

export function Checking() {
  return (
    <p role="status" className="py-6 text-center text-sm text-muted">
      Checking the link…
    </p>
  );
}

/** A heading the page moves focus to as it replaces another state of the page. */
export function Heading({ children, moveFocus }: { children: ReactNode; moveFocus: MoveFocus }) {
  const ref = useRef<HTMLHeadingElement>(null);
  // Decided as the heading mounts: later renders of the same state leave focus where the user put it.
  useEffect(() => {
    if (moveFocus.current) ref.current?.focus();
  }, [moveFocus]);
  return (
    <h2 ref={ref} tabIndex={-1} className="text-xl font-semibold outline-none sm:text-lg">
      {children}
    </h2>
  );
}

/** A link that can't be used, in words and the way back: Open Upheap sends a signed-out visitor on to sign-in. */
export function Notice({
  heading,
  moveFocus,
  children,
}: {
  heading: string;
  moveFocus: MoveFocus;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-3">
      <Heading moveFocus={moveFocus}>{heading}</Heading>
      {children}
      <Link to="/" className={`${authButtonClass} mt-1 flex items-center justify-center`}>
        Open Upheap
      </Link>
    </div>
  );
}
