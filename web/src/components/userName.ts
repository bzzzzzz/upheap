import type { UserRef } from "../api/schema";
import { useMe } from "../api/queries/me";

/**
 * How a user is named after "by": "you" for the viewer, as History says "You", or their display name. `undefined` when
 * there's no user to name.
 */
export function useUserName(user: UserRef | null | undefined): string | undefined {
  const { data: me } = useMe();
  if (!user) return undefined;
  return user.id === me?.user.id ? "you" : user.display_name;
}

/** The same name where it starts a sentence: "You" for the viewer, the display name as its owner wrote it. */
export function useUserNameAtStart(user: UserRef | null | undefined): string | undefined {
  const name = useUserName(user);
  return name === "you" ? "You" : name;
}
