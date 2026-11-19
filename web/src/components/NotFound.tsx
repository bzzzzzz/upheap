import { Link } from "@tanstack/react-router";

import { buttonClass } from "./buttons";

/**
 * A page for something that isn't there, or isn't the user's: the server answers both with `404`, so the words say
 * both (ux.md, Conflicts and errors). `teamsLink` adds the way to the user's teams, for a missing team or heap.
 */
export function NotFound({ heading, line, teamsLink }: { heading: string; line: string; teamsLink?: boolean }) {
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
      {teamsLink && (
        <Link to="/teams" className="text-sm text-muted underline">
          See your teams
        </Link>
      )}
    </div>
  );
}
