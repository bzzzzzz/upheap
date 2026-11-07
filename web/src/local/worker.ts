// The local build's backend: SQLite-wasm with its database file in OPFS, and the Rust backend compiled to wasm
// (crates/local), answering the page's API requests (design/local-build.md).
//
// `opfs-sahpool` keeps the database in the Origin Private File System without the COOP and COEP headers GitHub
// Pages can't send, but only inside a worker.
import sqlite3InitModule from "@sqlite.org/sqlite-wasm";

import init, { Backend } from "./backend/local.js";
import type { WorkerRequest, WorkerResponse } from "./protocol.ts";
import { installSqlite } from "./sqlite.ts";

const DATABASE_FILE = "/upheap.sqlite3";

async function openBackend(): Promise<Backend> {
  const sqlite3 = await sqlite3InitModule();
  const pool = await sqlite3.installOpfsSAHPoolVfs({ name: "upheap" });
  installSqlite(sqlite3, new pool.OpfsSAHPoolDb(DATABASE_FILE));
  await init();
  return Backend.open();
}

const backend = openBackend();

// Requests run one after the other, each to its end: the backend has one connection, and a transaction must not
// see another request's statements in between.
let queue: Promise<void> = Promise.resolve();

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  queue = queue.then(() => answer(request));
};

async function answer(request: WorkerRequest): Promise<void> {
  let reply: WorkerResponse;
  try {
    const response = await (
      await backend
    ).handle(request.method, request.uri, request.headers, new Uint8Array(request.body));
    const body = response.body.slice().buffer;
    reply = { id: request.id, status: response.status, headers: response.headers, body };
    response.free();
  } catch (error) {
    console.error("local backend failed", error);
    reply = { id: request.id, failure: String(error) };
  }
  if ("body" in reply) {
    self.postMessage(reply, { transfer: [reply.body] });
  } else {
    self.postMessage(reply);
  }
}
