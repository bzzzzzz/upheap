import { useMatch, useSearch } from "@tanstack/react-router";

import type { Me } from "../api/schema";
import { teamName } from "./teamName";

/** Which of the pages across heaps is on screen, and the team it is limited to, if any. */
export interface AcrossHeaps {
  view: "open" | "closed" | "activity";
  team: string | undefined;
}

/**
 * The page across heaps on screen (ux.md, All items): the open list at `/all`, its closed items or its activity. None
 * on any other page.
 */
export function useAcrossHeaps(): AcrossHeaps | undefined {
  const open = useMatch({ from: "/app/all", shouldThrow: false }) !== undefined;
  const closed = useMatch({ from: "/app/all/closed", shouldThrow: false }) !== undefined;
  const activity = useMatch({ from: "/app/all/activity", shouldThrow: false }) !== undefined;
  const openTeam = useSearch({ from: "/app/all", shouldThrow: false })?.team;
  const closedTeam = useSearch({ from: "/app/all/closed", shouldThrow: false })?.team;
  const activityTeam = useSearch({ from: "/app/all/activity", shouldThrow: false })?.team;
  if (open) return { view: "open", team: openTeam };
  if (closed) return { view: "closed", team: closedTeam };
  if (activity) return { view: "activity", team: activityTeam };
  return undefined;
}

/**
 * The team a page across heaps is limited to, when it is one of several the person is in: only then does the heap
 * switcher have a row for it.
 */
export function acrossTeam(me: Me, team: string | undefined) {
  return me.teams.length > 1 ? me.teams.find((t) => t.slug === team) : undefined;
}

/** What the heap switcher's row for the pages across heaps reads: "All heaps", or "All <team> items". */
export function acrossLabel(me: Me, team: string | undefined): string {
  const chosen = acrossTeam(me, team);
  return chosen ? `All ${teamName(chosen, me)} items` : "All heaps";
}

/** The line under a page's heading that says whose heaps it covers, ending with `rest`. */
export function acrossSubtitle(me: Me | undefined, team: string | undefined, rest: string): string {
  const chosen = me?.teams.find((t) => t.slug === team);
  if (!me || !chosen) return `All your heaps, ${rest}`;
  return `${chosen.personal ? "Your personal team" : teamName(chosen, me)}'s heaps, ${rest}`;
}
