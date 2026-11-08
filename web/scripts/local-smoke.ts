// Smoke test of the local build's backend in Node: the wasm from src/local/backend (npm run build:backend) on
// in-memory SQLite-wasm, through the same SQLite side the worker uses. CI runs it; it exits non-zero on a failure.
//
//   node scripts/local-smoke.ts
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import sqlite3InitModule from "@sqlite.org/sqlite-wasm";

import init, { Backend } from "../src/local/backend/local.js";
import { countDatabase, countFile, FileRefused } from "../src/local/databaseFile.ts";
import { installSqlite } from "../src/local/sqlite.ts";

type Answer = { status: number; body: unknown };

const sqlite3 = await sqlite3InitModule();
const db = new sqlite3.oo1.DB(":memory:");
installSqlite(sqlite3, db);
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
assert.equal(field(me.body, "user", "username"), "me", "the local user's username");
assert.equal(field(me.body, "teams", 0, "slug"), "me", "the local user's team");
assert.equal(field(me.body, "teams", 0, "heaps", 0, "slug"), "personal", "the local user's first heap");
const heap = "/api/v1/teams/me/heaps/personal";
assert.deepEqual(field(me.body, "allowed_actions"), [], "no signing out or new teams");
const team = await send("GET", "/api/v1/teams/me");
assert.deepEqual(field(team.body, "allowed_actions"), ["create_heap"], "the team offers only new heaps");

for (const [title, effort, impact] of [
  ["Big and dull", "xl", "s"],
  ["Small and valuable", "s", "xl"],
]) {
  const created = await send("POST", `${heap}/items`, { title, need: "expected", effort, impact });
  assert.equal(created.status, 201, `POST an item: ${JSON.stringify(created.body)}`);
}
const list = await send("GET", `${heap}/items`, undefined, ["time-zone", "Europe/Berlin"]);
assert.equal(list.status, 200, "GET the ranked list");
assert.equal(field(list.body, "ranked", 0, "item", "title"), "Small and valuable", "items are ranked");

const tag = await send("POST", `${heap}/tags`, { name: "home" });
assert.equal(tag.status, 201, "POST a tag");
const again = await send("POST", `${heap}/tags`, { name: "home" });
assert.equal(again.status, 409, "a duplicate tag");
assert.equal(field(again.body, "code"), "tag_name_taken");

const key = field(list.body, "ranked", 0, "item", "key");
const item = await send("GET", `${heap}/items/${String(key)}`);
assert.equal(field(item.body, "title"), "Small and valuable", "an item by its key");
const missing = await send("GET", `${heap}/items/zzzz9`);
assert.equal(missing.status, 404, "an unknown item");
assert.equal(field(missing.body, "code"), "not_found");
const register = await send("POST", "/api/v1/auth/register", {
  email: "a@b.c",
  username: "ab",
  display_name: "A",
  password: "x",
});
assert.equal(register.status, 404, "no registration without accounts");

const reopened = await Backend.open();
const meAgain = await reopened.handle("GET", "/api/v1/me", [], new Uint8Array());
const user = JSON.parse(new TextDecoder().decode(meAgain.body)) as { user: { id: string } };
assert.equal(user.user.id, field(me.body, "user", "id"), "opening again finds the same user");

// The worker's count, of the open database and of a copy of it, which opens in memory beside it.
const counts = countDatabase(db);
assert.equal(counts.heaps, 1, "count: the first heap");
assert.equal(counts.items, 2, "count: the items added");
assert.ok(counts.lastChangedAt !== null && counts.lastChangedAt > 0, "count: the newest audit entry");
assert.deepEqual(countFile(sqlite3, sqlite3.capi.sqlite3_js_db_export(db)), counts, "count: a copy holds the same");
assert.throws(() => countFile(sqlite3, new TextEncoder().encode("not a database")), FileRefused, "count: not SQLite");
const other = new sqlite3.oo1.DB(":memory:");
other.exec("CREATE TABLE notes (body TEXT)");
assert.throws(() => countFile(sqlite3, sqlite3.capi.sqlite3_js_db_export(other)), FileRefused, "count: not Upheap");

console.log("local backend: all checks passed");
