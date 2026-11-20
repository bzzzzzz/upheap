import { Users } from "lucide-react";

/**
 * Marks a team's tag or parameter, where one shows among a heap's own: the team icon after the name, and for a screen
 * reader ", team tag" or ", team parameter".
 */
export function TeamMark({ of }: { of: "tag" | "parameter" }) {
  return (
    <>
      <Users aria-hidden="true" className="ml-1 inline size-3.5 align-[-2px] text-muted" />
      <span className="sr-only">, team {of}</span>
    </>
  );
}
