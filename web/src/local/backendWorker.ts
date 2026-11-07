// The page's side of the backend's worker, which both the API client's `fetch` (fetch.ts) and the local database's
// export and import (database.ts) talk to. The worker starts with the first message or subscriber.
import type { WorkerMessage, WorkerReply, WorkerRequest, WorkerStatus } from "./protocol.ts";

/** Each kind of request without its `id`, which `send` gives it. */
type WithoutId<T> = T extends unknown ? Omit<T, "id"> : never;
type Outgoing = WithoutId<WorkerRequest>;

type Pending = { resolve: (reply: WorkerReply) => void; reject: (error: Error) => void };

let worker: Worker | undefined;
const pending = new Map<number, Pending>();
let nextId = 1;
let status: WorkerStatus = "starting";
const listeners = new Set<() => void>();

function startWorker(): Worker {
  const started = new Worker(new URL("./worker.ts", import.meta.url), { type: "module", name: "upheap-backend" });
  started.onmessage = (event: MessageEvent<WorkerMessage>) => {
    const message = event.data;
    if (message.kind === "status") {
      status = message.status;
      listeners.forEach((listener) => listener());
      return;
    }
    const waiting = pending.get(message.id);
    pending.delete(message.id);
    waiting?.resolve(message);
  };
  started.onerror = (event) => {
    const error = new TypeError(`The local backend stopped: ${event.message}`);
    for (const waiting of pending.values()) {
      waiting.reject(error);
    }
    pending.clear();
  };
  return started;
}

/** Sends `request` to the worker and resolves with its reply. `transfer` moves buffers instead of copying them. */
export function send(request: Outgoing, transfer: Transferable[] = []): Promise<WorkerReply> {
  worker ??= startWorker();
  const message = { ...request, id: nextId++ } as WorkerRequest;
  const target = worker;
  return new Promise((resolve, reject) => {
    pending.set(message.id, { resolve, reject });
    target.postMessage(message, { transfer });
  });
}

/** Calls `listener` whenever the status changes, for `useSyncExternalStore`. Returns the unsubscribe function. */
export function subscribeToStatus(listener: () => void): () => void {
  worker ??= startWorker();
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function workerStatus(): WorkerStatus {
  return status;
}
