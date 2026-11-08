// The local build's database for the web app (`#local-database`): keeping it, exporting, importing and counting it,
// and whether another tab has it. The server build resolves `#local-database` to src/api/noLocalDatabase.ts, which has
// none (package.json `imports`).
import { send, subscribeToStatus, workerStatus } from "./backendWorker.ts";
import type { DatabaseCounts, WorkerStatus } from "./protocol.ts";

export type { DatabaseCounts };

export type LocalDatabase = {
  /**
   * Asks the browser to keep the data rather than clear it when space runs low, unless it already does. Resolves
   * whether it will.
   */
  keep(): Promise<boolean>;
  /** The database file, a normal SQLite file with the server's schema. */
  exportFile(): Promise<Blob>;
  /** Replaces the database with `file`. Rejects with a message fit to show, and then nothing has changed. */
  importFile(file: Blob): Promise<void>;
  /**
   * What the database holds, or with `file` what that file holds, leaving the database as it is. Rejects with a
   * message fit to show when `file` isn't an Upheap database, as `importFile` would.
   */
  count(file?: Blob): Promise<DatabaseCounts>;
  /** For `useSyncExternalStore`: `waiting` while another tab has the database. */
  subscribe(listener: () => void): () => void;
  status(): WorkerStatus;
};

async function keep(): Promise<boolean> {
  // Not in an insecure context, where there's no OPFS either.
  if (!("storage" in navigator)) {
    return false;
  }
  return (await navigator.storage.persisted()) || navigator.storage.persist();
}

async function exportFile(): Promise<Blob> {
  const reply = await send({ kind: "export" });
  if (reply.kind !== "export") {
    throw new Error(reply.kind === "failure" ? reply.message : `unexpected reply ${reply.kind}`);
  }
  return new Blob([reply.file], { type: "application/vnd.sqlite3" });
}

async function importFile(file: Blob): Promise<void> {
  const bytes = await file.arrayBuffer();
  const reply = await send({ kind: "import", file: bytes }, [bytes]);
  if (reply.kind !== "import") {
    throw new Error(reply.kind === "failure" ? reply.message : `unexpected reply ${reply.kind}`);
  }
}

async function count(file?: Blob): Promise<DatabaseCounts> {
  const bytes = await file?.arrayBuffer();
  const reply = await send({ kind: "count", file: bytes }, bytes ? [bytes] : []);
  if (reply.kind !== "count") {
    throw new Error(reply.kind === "failure" ? reply.message : `unexpected reply ${reply.kind}`);
  }
  return { heaps: reply.heaps, items: reply.items, lastChangedAt: reply.lastChangedAt };
}

export const localDatabase: LocalDatabase | null = {
  keep,
  exportFile,
  importFile,
  count,
  subscribe: subscribeToStatus,
  status: workerStatus,
};
