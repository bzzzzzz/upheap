import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { buttonClass } from "./buttons";

/**
 * A page for something that isn't there, or isn't the user's: the server answers both with `404`, so the words say
 * both (ux.md, Conflicts and errors). `children` goes under the button.
 */
export function NotFound({ heading, line, children }: { heading: string; line: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 py-16 text-center">
      <p className="text-xs text-muted">Not found</p>
      <h1 className="text-lg font-semibold">{heading}</h1>
      <p className="text-sm text-ink-soft">{line}</p>
      <Link
        to="/waiting"
        className={`${buttonClass} mt-4 min-h-10 rounded-[10px] bg-accent px-4 font-semibold text-stone-950 hover:brightness-95`}
      >
        Go to Waiting for me
      </Link>
      {children}
    </div>
  );
}

function TeamsLink() {
  return (
    <Link to="/teams" className="text-sm text-muted underline">
      See your teams
    </Link>
  );
}

/** A team the API answers `404` for. */
export function TeamNotFound() {
  return (
    <NotFound heading="This team isn't here" line="It doesn't exist, or you're not in it.">
      <TeamsLink />
    </NotFound>
  );
}

/** A heap the API answers `404` for. */
export function HeapNotFound() {
  return (
    <NotFound heading="This heap isn't here" line="It doesn't exist, or you're not in its team.">
      <TeamsLink />
    </NotFound>
  );
}
