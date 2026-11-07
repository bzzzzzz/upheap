// The local build's `fetch` for the API client (`#api-fetch`): requests go to the backend's worker instead of the
// network.
import { send } from "./backendWorker.ts";

/** Statuses whose responses can't have a body, which the `Response` constructor refuses one for. */
const NULL_BODY_STATUSES = new Set([101, 204, 205, 304]);

export async function apiFetch(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const headers: string[] = [];
  request.headers.forEach((value, name) => headers.push(name, value));
  const body = await request.arrayBuffer();
  const reply = await send({ kind: "http", method: request.method, uri: url.pathname + url.search, headers, body }, [
    body,
  ]);
  if (reply.kind !== "http") {
    // Like a network error: openapi-fetch rethrows it, and TanStack Query retries.
    throw new TypeError(`The local backend failed: ${reply.kind === "failure" ? reply.message : reply.kind}`);
  }
  const responseHeaders = new Headers();
  for (let i = 0; i + 1 < reply.headers.length; i += 2) {
    responseHeaders.append(reply.headers[i] ?? "", reply.headers[i + 1] ?? "");
  }
  const responseBody = NULL_BODY_STATUSES.has(reply.status) ? null : reply.body;
  return new Response(responseBody, { status: reply.status, headers: responseHeaders });
}
