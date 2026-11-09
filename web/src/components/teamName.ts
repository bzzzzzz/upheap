import type { HeapRef, Me } from "../api/client";
import { soleTeam, useMe } from "../api/queries";

type NamedTeam = { name: string; personal: boolean };

/**
 * Whether the team shows as "<display name>'s team" rather than its stored name (design/decisions.md, 31): a personal
 * team among others, whose stored name is its owner's and reads as a person. Alone, as in the local build, there is no
 * list of teams to tell it from, so it keeps its own name ("My heaps").
 */
export function namedForOwner(team: NamedTeam, me: Me): boolean {
  return team.personal && soleTeam(me) === undefined;
}

/** How the web app names a team. Display only: the stored name is unchanged. */
export function teamName(team: NamedTeam, me: Me): string {
  return namedForOwner(team, me) ? `${me.user.display_name}'s team` : team.name;
}

/** `teamName` for a component that doesn't hold `me`; until it loads, the stored name. */
export function useTeamName(team: NamedTeam): string {
  const { data: me } = useMe();
  return me ? teamName(team, me) : team.name;
}

/**
 * The team a heap's title names after its own name ("Launch · Acme"): a shared team's label, or `undefined` for a
 * personal heap, which needs no team, or a team the user isn't in.
 */
export function titleTeam(me: Me | undefined, heapRef: HeapRef | undefined): string | undefined {
  const team = me?.teams.find((t) => t.slug === heapRef?.team);
  return me && team && !team.personal ? teamName(team, me) : undefined;
}
