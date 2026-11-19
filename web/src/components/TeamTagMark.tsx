import { Users } from "lucide-react";

/**
 * Marks a team tag, in the tag picker's options and the list filter's toggles: the team icon after the name, and for a
 * screen reader ", team tag".
 */
export function TeamTagMark() {
  return (
    <>
      <Users aria-hidden="true" className="ml-1 inline size-3.5 align-[-2px] text-muted" />
      <span className="sr-only">, team tag</span>
    </>
  );
}
