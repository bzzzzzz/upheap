// Messages between the page (backendWorker.ts) and the backend's worker (worker.ts). Each request message gets
// exactly one reply with the same `id`; status messages come on their own.

export type WorkerRequest =
  | {
      id: number;
      kind: "http";
      method: string;
      /** Path and query, such as `/api/v1/me`. */
      uri: string;
      /** Names and values, alternating. */
      headers: string[];
      body: ArrayBuffer;
    }
  /** The database file, for a download. */
  | { id: number; kind: "export" }
  /** Replaces the database with `file`, after checking it opens. */
  | { id: number; kind: "import"; file: ArrayBuffer };

export type WorkerReply =
  | { id: number; kind: "http"; status: number; headers: string[]; body: ArrayBuffer }
  | { id: number; kind: "export"; file: ArrayBuffer }
  | { id: number; kind: "import" }
  /** The backend couldn't start or couldn't do what was asked at all. `message` is fit to show. */
  | { id: number; kind: "failure"; message: string };

/**
 * Where the worker is: `waiting` while another tab has the database, which `opfs-sahpool` lets only one
 * connection open, and `ready` once it has opened it.
 */
export type WorkerStatus = "starting" | "waiting" | "ready";

export type WorkerMessage = WorkerReply | { kind: "status"; status: WorkerStatus };
