import { onlineManager, useQueryClient } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";

import { serverReachability } from "../api/client";

/**
 * A strip at the top of the app shell while the browser is offline, or online but the server can't be reached
 * (ux.md, Conflicts and errors). Offline wins when both hold. It says nothing when the connection returns.
 */
export function ConnectionBanner() {
  const queryClient = useQueryClient();
  const online = useSyncExternalStore(onlineManager.subscribe, () => onlineManager.isOnline());
  const unreachable = useSyncExternalStore(serverReachability.subscribe, serverReachability.get);
  if (online && !unreachable) return null;

  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-x-1 border-b border-accent/40 bg-accent-soft px-4 py-1.5 text-sm text-accent-ink"
    >
      <span>
        <b className="font-semibold">{online ? "Can't reach the server." : "You're offline."}</b>
        <span className="sm:hidden"> Won't save.</span>
        <span className="hidden sm:inline">
          {online ? " Changes won't save until it's back." : " Changes won't save until the connection is back."}
        </span>
      </span>
      {online && (
        <button type="button" onClick={() => void queryClient.invalidateQueries()} className="font-medium underline">
          Try now
        </button>
      )}
    </div>
  );
}
