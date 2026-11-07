// Smoke test of the local build's backend in Node: the wasm from src/local/backend (npm run build:backend) on
// in-memory SQLite-wasm, through the same SQLite side the worker uses. CI runs it; it exits non-zero on a failure.
//
//   node scripts/local-smoke.ts
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import sqlite3InitModule from "@sqlite.org/sqlite-wasm";

import init, { Backend } from "../src/local/backend/local.js";
import { installSqlite } from "../src/local/sqlite.ts";

type Answer = { status: number; body: unknown };

const sqlite3 = await sqlite3InitModule();
installSqlite(sqlite3, new sqlite3.oo1.DB(":memory:"));
await init({ module_or_path: await readFile(new URL("../src/local/backend/local_bg.wasm", import.meta.url)) });

const backend = await Backend.open();

async function send(method: string, uri: string, body?: unknown, headers: string[] = ["x-csrf-protection", "1"]) {
  const bytes = new TextEncoder().encode(body === undefined ? "" : JSON.stringify(body));
  const json = body === undefined ? [] : ["content-type", "application/json"];
  const response = await backend.handle(method, uri, [...headers, ...json], bytes);
  const text = new TextDecoder().decode(response.body);
  const answer: Answer = { status: response.status, body: text ? JSON.parse(text) : null };
  response.free();
  return answer;
}

const field = (value: unknown, ...path: (string | number)[]): unknown =>
  path.reduce<unknown>((at, key) => (at as Record<string | number, unknown> | null)?.[key], value);

const me = await send("GET", "/api/v1/me");
assert.equal(me.status, 200, "GET /me");
assert.equal(field(me.body, "user", "display_name"), "Me", "the local user");
const heap = field(me.body, "teams", 0, "heaps", 0, "id");
assert.equal(typeof heap, "string", "the local user's first heap");

for (const [title, effort, impact] of [
  ["Big and dull", "xl", "s"],
  ["Small and valuable", "s", "xl"],
]) {
  const created = await send("POST", `/api/v1/heaps/${heap}/items`, { title, effort, impact });
  assert.equal(created.status, 201, `POST an item: ${JSON.stringify(created.body)}`);
}
const list = await send("GET", `/api/v1/heaps/${heap}/items`, undefined, ["time-zone", "Europe/Berlin"]);
assert.equal(list.status, 200, "GET the ranked list");
assert.equal(field(list.body, "ranked", 0, "item", "title"), "Small and valuable", "items are ranked");

const tag = await send("POST", `/api/v1/heaps/${heap}/tags`, { name: "home" });
assert.equal(tag.status, 201, "POST a tag");
const again = await send("POST", `/api/v1/heaps/${heap}/tags`, { name: "home" });
assert.equal(again.status, 409, "a duplicate tag");
assert.equal(field(again.body, "code"), "tag_name_taken");

const missing = await send("GET", "/api/v1/items/00000000-0000-0000-0000-000000000000");
assert.equal(missing.status, 404, "an unknown item");
assert.equal(field(missing.body, "code"), "not_found");
const register = await send("POST", "/api/v1/auth/register", { email: "a@b.c", display_name: "A", password: "x" });
assert.equal(register.status, 404, "no registration without accounts");

const reopened = await Backend.open();
const meAgain = await reopened.handle("GET", "/api/v1/me", [], new Uint8Array());
const user = JSON.parse(new TextDecoder().decode(meAgain.body)) as { user: { id: string } };
assert.equal(user.user.id, field(me.body, "user", "id"), "opening again finds the same user");

console.log("local backend: all checks passed");
