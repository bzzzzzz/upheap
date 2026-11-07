// Runs the spike headless: in-memory SQLite-wasm in Node, the backend wasm from `pkg/`. CI runs it too, so it
// checks what the spike proved and exits non-zero when something broke.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import sqlite3InitModule from "@sqlite.org/sqlite-wasm";
import { installShim } from "./shim.mjs";
import init, { spike, spike_http } from "../pkg/local.js";

const sqlite3 = await sqlite3InitModule();
const db = new sqlite3.oo1.DB(":memory:");
installShim(sqlite3, db);
await init({ module_or_path: await readFile(new URL("../pkg/local_bg.wasm", import.meta.url)) });

const backend = JSON.parse(await spike());
console.log(JSON.stringify(backend, null, 2));
assert.ok(backend.migrations_applied > 0, "migrations ran");
assert.equal(backend.same_user_reopened, true, "opening the local user again finds the same user");
assert.deepEqual(
  backend.ranked.map((entry) => entry.title),
  ["Small and valuable", "Middling", "Big and dull"],
  "items are ranked",
);
assert.equal(backend.transactions.rows_after_drop, 0, "a dropped transaction rolls back");
assert.equal(backend.transactions.rows_after_commit, 1, "a committed transaction stays");
assert.match(backend.transactions.duplicate_key, /^Duplicate\(/, "a duplicate key is StoreError::Duplicate");

const http = JSON.parse(await spike_http());
console.log(JSON.stringify(http, null, 2));
assert.equal(http.me[0], 200, "GET /me");
assert.equal(http.create_item[0], 201, "POST an item");
assert.equal(http.ranked_list[0], 200, "GET the ranked list");
assert.equal(http.missing_item[0], 404, "GET an unknown item");
console.log("spike: all checks passed");
