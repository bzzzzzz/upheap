// The local build's `fetch` for the API client (`#api-fetch`): requests go to the backend's worker instead of the network. The
// worker starts with the first request.
import type { WorkerRequest, WorkerResponse } from "./protocol.ts";

/** Statuses whose responses can't have a body, which the `Response` constructor refuses one for. */
const NULL_BODY_STATUSES = new Set([101, 204, 205, 304]);

type Pending = { resolve: (response: Response) => void; reject: (error: Error) => void };

let worker: Worker | undefined;
const pending = new Map<number, Pending>();
let nextId = 1;

function startWorker(): Worker {
  const started = new Worker(new URL("./worker.ts", import.meta.url), { type: "module", name: "upheap-backend" });
  started.onmessage = (event: MessageEvent<WorkerResponse>) => {
    const reply = event.data;
    const waiting = pending.get(reply.id);
    pending.delete(reply.id);
    if (!waiting) {
      return;
    }
    if ("failure" in reply) {
      // Like a network error: openapi-fetch rethrows it, and TanStack Query retries.
      waiting.reject(new TypeError(`The local backend failed: ${reply.failure}`));
      return;
    }
    const headers = new Headers();
    for (let i = 0; i + 1 < reply.headers.length; i += 2) {
      headers.append(reply.headers[i] ?? "", reply.headers[i + 1] ?? "");
    }
    const body = NULL_BODY_STATUSES.has(reply.status) ? null : reply.body;
    waiting.resolve(new Response(body, { status: reply.status, headers }));
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

export async function apiFetch(request: Request): Promise<Response> {
  worker ??= startWorker();
  const url = new URL(request.url);
  const headers: string[] = [];
  request.headers.forEach((value, name) => headers.push(name, value));
  const message: WorkerRequest = {
    id: nextId++,
    method: request.method,
    uri: url.pathname + url.search,
    headers,
    body: await request.arrayBuffer(),
  };
  const target = worker;
  return new Promise((resolve, reject) => {
    pending.set(message.id, { resolve, reject });
    target.postMessage(message, { transfer: [message.body] });
  });
}
