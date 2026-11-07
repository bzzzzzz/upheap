// Messages between the page (fetch.ts) and the backend's worker (worker.ts). One request message gets exactly one
// response message with the same `id`.

export type WorkerRequest = {
  id: number;
  method: string;
  /** Path and query, such as `/api/v1/me`. */
  uri: string;
  /** Names and values, alternating. */
  headers: string[];
  body: ArrayBuffer;
};

export type WorkerResponse =
  | { id: number; status: number; headers: string[]; body: ArrayBuffer }
  /** The backend couldn't start or couldn't handle the request at all. */
  | { id: number; failure: string };
