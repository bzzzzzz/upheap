import type { Me } from "../api/client";
import { useMe } from "../api/queries";

/**
 * How the web app names a team. A personal team is stored under its owner's name, which reads as a person in a list of
 * teams, so it shows as "<display name>'s team"; a shared team shows its own name. Display only: the stored name is
 * unchanged.
 */
export function teamName(team: { name: string; personal: boolean }, me: Me): string {
  return team.personal ? `${me.user.display_name}'s team` : team.name;
}

/** `teamName` for a component that doesn't hold `me`; until it loads, the stored name. */
export function useTeamName(team: { name: string; personal: boolean }): string {
  const { data: me } = useMe();
  return me ? teamName(team, me) : team.name;
}
