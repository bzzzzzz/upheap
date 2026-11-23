// Typed API client. Types come from the generated schema only; never write API types by hand. Schema types are
// imported from `./schema` under their own names.
import createClient, { type Middleware } from "openapi-fetch";

import type { MergeReach, ParameterMergeReach, paths, Problem } from "./schema";

/** The list view parameters of the ranked list, without `as_of`. */
export type ListViewQuery = Omit<
  NonNullable<paths["/api/v1/teams/{team}/heaps/{heap}/items"]["get"]["parameters"]["query"]>,
  "as_of"
>;

/** The view parameters of the list across heaps, without `as_of`. */
export type AcrossHeapsQuery = Omit<NonNullable<paths["/api/v1/me/items"]["get"]["parameters"]["query"]>, "as_of">;

/**
 * A heap as paths name it: its team's slug and its own (architecture.md, Handles). It is exactly the path parameters
 * of the heap's API routes, so it goes into `params.path` as it is.
 */
export type HeapRef = { team: string; heap: string };

/** A failed API call. Branch on `code`, never on `message`. */
export class ApiProblem extends Error {
  readonly status: number;
  readonly code: string;
  readonly field: string | undefined;
  /** A value of `field` the server would accept, such as `my-team` for `My-Team` (`invalid_handle`). */
  readonly hint: string | undefined;
  /** The server's request ID, on a `5xx`, for whoever runs the server to look up. */
  readonly requestId: string | undefined;
  /** What a team tag's name would merge, on `tag_merge_unconfirmed`. */
  readonly mergeReach: MergeReach | undefined;
  /** Seconds to wait before trying again, from the `Retry-After` header of a `429`. */
  readonly retryAfter: number | undefined;
  /** What making a heap parameter a team parameter would merge, on `parameter_merge_unconfirmed`. */
  readonly parameterMergeReach: ParameterMergeReach | undefined;

  constructor(
    status: number,
    code: string,
    message: string,
    field?: string,
    hint?: string,
    requestId?: string,
    mergeReach?: MergeReach,
    retryAfter?: number,
    parameterMergeReach?: ParameterMergeReach,
  ) {
    super(message);
    this.status = status;
    this.code = code;
    this.field = field;
    this.hint = hint;
    this.requestId = requestId;
    this.mergeReach = mergeReach;
    this.retryAfter = retryAfter;
    this.parameterMergeReach = parameterMergeReach;
  }
}

function isProblem(value: unknown): value is Problem {
  return typeof value === "object" && value !== null && "code" in value && "title" in value;
}

const standardHeaders: Middleware = {
  onRequest({ request }) {
    // Proves the request comes from this app (see the server's CSRF check).
    request.headers.set("X-CSRF-Protection", "1");
    // The server formats dates and fuzzy labels in the device's timezone.
    request.headers.set("Time-Zone", Intl.DateTimeFormat().resolvedOptions().timeZone);
    return request;
  },
};

let unreachable = false;
const reachabilityListeners = new Set<() => void>();

function setUnreachable(value: boolean) {
  if (unreachable === value) return;
  unreachable = value;
  reachabilityListeners.forEach((listener) => listener());
}

/**
 * Whether the last call couldn't reach the server: set by a call that fails with `unreachable`, cleared by any response,
 * error statuses included. For `useSyncExternalStore`.
 */
export const serverReachability = {
  subscribe(listener: () => void) {
    reachabilityListeners.add(listener);
    return () => void reachabilityListeners.delete(listener);
  },
  get: () => unreachable,
};

/**
 * A reverse proxy answers `502`, `503` or `504` itself, with no problem body, when the server process is down. That
 * is the server being out of reach, not a server error.
 */
function isProxyFailure(response: Response): boolean {
  return (
    [502, 503, 504].includes(response.status) &&
    !response.headers.get("content-type")?.startsWith("application/problem+json")
  );
}

/** Turns a `fetch` that threw into a problem. Nothing in the app aborts a request, so every throw is a network failure. */
const networkProblems: Middleware = {
  onError() {
    if (!navigator.onLine) {
      return new ApiProblem(0, "offline", "You're offline.");
    }
    setUnreachable(true);
    return new ApiProblem(0, "unreachable", "Can't reach the server.");
  },
  onResponse({ response }) {
    setUnreachable(isProxyFailure(response));
  },
};

export const api = createClient<paths>({
  baseUrl: window.location.origin,
  credentials: "same-origin",
  // List parameters are comma-separated (`tag=a,b`), as the spec declares with `explode: false`.
  querySerializer: { array: { style: "form", explode: false } },
});
api.use(standardHeaders, networkProblems);

/** Returns the response data, or throws an `ApiProblem`. */
export function unwrap<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.data !== undefined && result.response.ok) {
    return result.data;
  }
  throw toProblem(result.error, result.response);
}

/** Throws an `ApiProblem` unless the response succeeded. For endpoints without a body. */
export function ensureOk(result: { error?: unknown; response: Response }): void {
  if (!result.response.ok) {
    throw toProblem(result.error, result.response);
  }
}

function toProblem(error: unknown, response: Response): ApiProblem {
  const requestId = response.status >= 500 ? (response.headers.get("x-request-id") ?? undefined) : undefined;
  const retryHeader = response.headers.get("retry-after");
  const retryAfter = retryHeader === null ? undefined : Number(retryHeader);
  if (isProblem(error)) {
    return new ApiProblem(
      error.status,
      error.code,
      error.title,
      error.field ?? undefined,
      error.hint ?? undefined,
      requestId,
      error.merge_reach ?? undefined,
      retryAfter,
      error.parameter_merge_reach ?? undefined,
    );
  }
  if (isProxyFailure(response)) {
    return new ApiProblem(response.status, "unreachable", "Can't reach the server.", undefined, undefined, requestId);
  }
  return new ApiProblem(
    response.status,
    "unexpected_response",
    `The server answered ${response.status}.`,
    undefined,
    undefined,
    requestId,
  );
}

/**
 * Whether the request had no live session. By `code`: a wrong password at sign-in is a `401` too
 * (`invalid_credentials`), and isn't this.
 */
export function isUnauthenticated(error: unknown): boolean {
  return error instanceof ApiProblem && error.code === "unauthenticated";
}

/** Whether the server answered `404`: the team, heap or item isn't there, or isn't the user's. */
export function isNotFound(error: unknown): boolean {
  return error instanceof ApiProblem && error.status === 404;
}

/** The request ID of a failed call, if the server sent one. */
export function requestIdOf(error: Error): string | undefined {
  return error instanceof ApiProblem ? error.requestId : undefined;
}
