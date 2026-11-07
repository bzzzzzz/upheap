// The local build's backend: SQLite-wasm with its database file in OPFS, and the Rust backend compiled to wasm
// (crates/local), answering the page's API requests and exporting and importing the database
// (design/local-build.md).
//
// `opfs-sahpool` keeps the database in the Origin Private File System without the COOP and COEP headers GitHub
// Pages can't send, but only inside a worker, and only one connection at a time: a second tab's pool would fail to
// open the files. So the worker first takes a Web Lock that it holds until its tab closes, and a second tab's
// worker waits for it.
import sqlite3InitModule, {
  type OpfsSAHPoolDatabase,
  type SAHPoolUtil,
  type Sqlite3Static,
} from "@sqlite.org/sqlite-wasm";

import init, { Backend } from "./backend/local.js";
import type { WorkerMessage, WorkerReply, WorkerRequest, WorkerStatus } from "./protocol.ts";
import { installSqlite } from "./sqlite.ts";

const DATABASE_FILE = "/upheap.sqlite3";
const LOCK_NAME = "upheap-database";
/** The first 16 bytes of every SQLite database file. */
const SQLITE_HEADER = "SQLite format 3\0";

/** An error whose message is fit to show as it is. */
class ImportRefused extends Error {}

/** The open database and the backend over it. */
type Opened = { db: OpfsSAHPoolDatabase; backend: Backend };

// Set by `start`, which every request waits for.
let sqlite3: Sqlite3Static;
let pool: SAHPoolUtil;
let opened: Opened | undefined;

function post(message: WorkerMessage, transfer: Transferable[] = []): void {
  self.postMessage(message, { transfer });
}

function announce(status: WorkerStatus): void {
  post({ kind: "status", status });
}

/** Resolves once this worker holds the database's lock, which it keeps until the worker ends with its tab. */
function takeLock(): Promise<void> {
  return new Promise((taken) => {
    const hold = () => {
      taken();
      return new Promise<never>(() => {});
    };
    void navigator.locks.request(LOCK_NAME, { ifAvailable: true }, (lock) => {
      if (lock) {
        return hold();
      }
      announce("waiting");
      return navigator.locks.request(LOCK_NAME, hold);
    });
  });
}

/** Opens the database file as it is now, runs the migrations and opens the backend over it. */
async function openDatabase(): Promise<Opened> {
  const db = new pool.OpfsSAHPoolDb(DATABASE_FILE);
  try {
    installSqlite(sqlite3, db);
    return { db, backend: await Backend.open() };
  } catch (error) {
    db.close();
    throw error;
  }
}

function closeDatabase(): void {
  opened?.backend.free();
  opened?.db.close();
  opened = undefined;
}

/**
 * Replaces the database with `file`. The current database is kept until the new one has opened, migrations
 * included, and put back if it doesn't.
 */
async function importDatabase(file: Uint8Array): Promise<void> {
  if (new TextDecoder().decode(file.subarray(0, SQLITE_HEADER.length)) !== SQLITE_HEADER) {
    throw new ImportRefused("This file isn't an Upheap database.");
  }
  const current = pool.exportFile(DATABASE_FILE);
  closeDatabase();
  try {
    await pool.importDb(DATABASE_FILE, file);
    checkUpheapDatabase();
    opened = await openDatabase();
  } catch (error) {
    await pool.importDb(DATABASE_FILE, current);
    opened = await openDatabase();
    throw error instanceof ImportRefused
      ? error
      : new ImportRefused(`Upheap couldn't open this file, so nothing changed: ${String(error)}`);
  }
}

/** Refuses a database that isn't sound or that no Upheap has migrated, before the migrations would fill it in. */
function checkUpheapDatabase(): void {
  const db = new pool.OpfsSAHPoolDb(DATABASE_FILE);
  try {
    const sound = db.selectValue("PRAGMA quick_check") === "ok";
    const migrated = db.selectValue("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'seaql_migrations'");
    if (!sound || migrated !== 1) {
      throw new ImportRefused("This file isn't an Upheap database.");
    }
  } catch (error) {
    throw error instanceof ImportRefused ? error : new ImportRefused("This file isn't an Upheap database.");
  } finally {
    db.close();
  }
}

async function start(): Promise<void> {
  await takeLock();
  sqlite3 = await sqlite3InitModule();
  pool = await sqlite3.installOpfsSAHPoolVfs({ name: "upheap" });
  await init();
  opened = await openDatabase();
  announce("ready");
}

const started = start();

async function answer(request: WorkerRequest): Promise<[WorkerReply, Transferable[]]> {
  await started;
  if (!opened) {
    throw new Error("the database isn't open");
  }
  switch (request.kind) {
    case "http": {
      const response = await opened.backend.handle(
        request.method,
        request.uri,
        request.headers,
        new Uint8Array(request.body),
      );
      const body = response.body.slice().buffer;
      const reply: WorkerReply = {
        id: request.id,
        kind: "http",
        status: response.status,
        headers: response.headers,
        body,
      };
      response.free();
      return [reply, [body]];
    }
    case "export": {
      const file = pool.exportFile(DATABASE_FILE).slice().buffer;
      return [{ id: request.id, kind: "export", file }, [file]];
    }
    case "import":
      await importDatabase(new Uint8Array(request.file));
      return [{ id: request.id, kind: "import" }, []];
  }
}

// Requests run one after the other, each to its end: the backend has one connection, a transaction must not see
// another request's statements in between, and an import must not swap the database under one.
let queue: Promise<void> = Promise.resolve();

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  queue = queue.then(async () => {
    try {
      const [reply, transfer] = await answer(request);
      post(reply, transfer);
    } catch (error) {
      if (!(error instanceof ImportRefused)) {
        console.error("local backend failed", error);
      }
      post({ id: request.id, kind: "failure", message: error instanceof Error ? error.message : String(error) });
    }
  });
};
