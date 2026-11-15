// Typed API client. Types come from the generated schema only; never write API types by hand. Schema types are
// imported from `./schema` under their own names.
import createClient, { type Middleware } from "openapi-fetch";

import type { paths, Problem } from "./schema";

/** The list view parameters of the ranked list, without `as_of`. */
export type ListViewQuery = Omit<
  NonNullable<paths["/api/v1/teams/{team}/heaps/{heap}/items"]["get"]["parameters"]["query"]>,
  "as_of"
>;

/** The view parameters of the list across heaps, without `as_of`. */
export type AcrossHeapsQuery = Omit<NonNullable<paths["/api/v1/me/items"]["get"]["parameters"]["query"]>, "as_of">;

/**
 * A heap as paths name it: its team's slug and its own (design/architecture.md, Handles). It is exactly the path parameters
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

  constructor(status: number, code: string, message: string, field?: string, hint?: string) {
    super(message);
    this.status = status;
    this.code = code;
    this.field = field;
    this.hint = hint;
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

export const api = createClient<paths>({
  baseUrl: window.location.origin,
  credentials: "same-origin",
  // List parameters are comma-separated (`tag=a,b`), as the spec declares with `explode: false`.
  querySerializer: { array: { style: "form", explode: false } },
});
api.use(standardHeaders);

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
  if (isProblem(error)) {
    return new ApiProblem(error.status, error.code, error.title, error.field ?? undefined, error.hint ?? undefined);
  }
  return new ApiProblem(response.status, "unexpected_response", `The server answered ${response.status}.`);
}

/**
 * Whether the request had no live session. By `code`: a wrong password at sign-in is a `401` too
 * (`invalid_credentials`), and isn't this.
 */
export function isUnauthenticated(error: unknown): boolean {
  return error instanceof ApiProblem && error.code === "unauthenticated";
}
