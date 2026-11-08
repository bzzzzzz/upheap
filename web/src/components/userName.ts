import type { UserRef } from "../api/client";
import { useMe } from "../api/queries";

/**
 * How a user is named after "by": "you" for the viewer, as History says "You", or their display name. `undefined` when
 * there's no user to name.
 */
export function useUserName(user: UserRef | null | undefined): string | undefined {
  const { data: me } = useMe();
  if (!user) return undefined;
  return user.id === me?.user.id ? "you" : user.display_name;
}
