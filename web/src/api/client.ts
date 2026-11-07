// Typed API client. Types come from the generated schema only; never write API types by hand.
import { apiFetch } from "#api-fetch";
import createClient, { type Middleware } from "openapi-fetch";

import type { components, paths } from "./schema";

export type Schemas = components["schemas"];
export type Item = Schemas["Item"];
export type RankedItem = Schemas["RankedItem"];
export type RankedList = Schemas["RankedList"];
export type ClosedItems = Schemas["ClosedItems"];
export type Me = Schemas["Me"];
export type Size = Schemas["Size"];
export type ItemState = Schemas["ItemState"];
export type ItemPatch = Schemas["ItemPatchRequest"];
export type NewItem = Schemas["CreateItemRequest"];
export type Tag = Schemas["Tag"];
export type TagList = Schemas["TagList"];
export type ItemTag = Schemas["ItemTag"];
export type ItemRef = Schemas["ItemRef"];
export type TagPatch = Schemas["TagPatchRequest"];
export type NewTag = Schemas["CreateTagRequest"];
export type StateMode = Schemas["StateMode"];
export type BlockedLayout = Schemas["BlockedLayout"];
export type ListSort = Schemas["ListSort"];
export type Team = Schemas["Team"];
export type TeamMember = Schemas["TeamMember"];
export type Role = Schemas["Role"];
export type Heap = Schemas["Heap"];
export type UserMatch = Schemas["UserMatch"];
export type User = Schemas["User"];
export type SavedView = Schemas["SavedView"];
export type ListViewInput = Schemas["ListViewInput"];
export type Parameter = Schemas["Parameter"];
export type ParameterList = Schemas["ParameterList"];
export type ParameterOption = Schemas["ParameterOption"];
export type ItemParameter = Schemas["ItemParameter"];
export type NewParameter = Schemas["CreateParameterRequest"];
export type ParameterPatch = Schemas["ParameterPatchRequest"];
export type AuditEntry = Schemas["AuditEntry"];
export type AuditAction = Schemas["AuditAction"];
export type AuditEntityType = Schemas["AuditEntityType"];
/** The list view parameters of the ranked list, without `as_of`. */
export type ListViewQuery = Omit<
  NonNullable<paths["/api/v1/heaps/{heap_id}/items"]["get"]["parameters"]["query"]>,
  "as_of"
>;
type Problem = Schemas["Problem"];

/** A failed API call. Branch on `code`, never on `message`. */
export class ApiProblem extends Error {
  readonly status: number;
  readonly code: string;
  readonly field: string | undefined;

  constructor(status: number, code: string, message: string, field?: string) {
    super(message);
    this.status = status;
    this.code = code;
    this.field = field;
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
  // The browser's, or the local build's worker (src/local/fetch.ts).
  ...(apiFetch ? { fetch: apiFetch } : {}),
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
    return new ApiProblem(error.status, error.code, error.title, error.field ?? undefined);
  }
  return new ApiProblem(response.status, "unexpected_response", `The server answered ${response.status}.`);
}

export function isUnauthenticated(error: unknown): boolean {
  return error instanceof ApiProblem && error.status === 401;
}
