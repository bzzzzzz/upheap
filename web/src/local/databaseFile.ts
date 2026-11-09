// Checking and counting an Upheap database file, for the worker's `import` and `count` (design/architecture.md,
// Local build). Apart from worker.ts so that the smoke test (scripts/local-smoke.ts) can run it in Node.
import type { Database, Sqlite3Static } from "@sqlite.org/sqlite-wasm";

import type { DatabaseCounts } from "./protocol.ts";

/** The first 16 bytes of every SQLite database file. */
const SQLITE_HEADER = "SQLite format 3\0";
const NOT_UPHEAP = "This file isn't an Upheap database.";

/** An error whose message is fit to show as it is. */
export class FileRefused extends Error {}

/** Refuses a file that doesn't start as every SQLite database does, before anything opens it. */
export function checkHeader(file: Uint8Array): void {
  if (new TextDecoder().decode(file.subarray(0, SQLITE_HEADER.length)) !== SQLITE_HEADER) {
    throw new FileRefused(NOT_UPHEAP);
  }
}

/** Refuses a database that isn't sound or that no Upheap has migrated, before the migrations would fill it in. */
export function checkUpheapDatabase(db: Database): void {
  try {
    const sound = db.selectValue("PRAGMA quick_check") === "ok";
    const migrated = db.selectValue("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'seaql_migrations'");
    if (!sound || migrated !== 1) {
      throw new FileRefused(NOT_UPHEAP);
    }
  } catch (error) {
    throw error instanceof FileRefused ? error : new FileRefused(NOT_UPHEAP);
  }
}

/** Every heap and item in `db`, archived and closed ones too, and when its newest audit entry was written. */
export function countDatabase(db: Database): DatabaseCounts {
  const [heaps, items, lastChangedAt] =
    db.selectArray(
      `SELECT (SELECT count(*) FROM heaps), (SELECT count(*) FROM items), (SELECT max(created_at) FROM audit_log)`,
    ) ?? [];
  return {
    heaps: Number(heaps),
    items: Number(items),
    lastChangedAt: lastChangedAt === null || lastChangedAt === undefined ? null : Number(lastChangedAt),
  };
}

/**
 * Counts the database in `file` without storing it anywhere: it opens read-only in memory, not in an OPFS slot.
 * Refuses, with a message fit to show, a file that import would refuse for not being an Upheap database, or one it
 * couldn't read at all.
 */
export function countFile(sqlite3: Sqlite3Static, file: Uint8Array): DatabaseCounts {
  checkHeader(file);
  // Shorter than its own header, so no database; and bytes 18 and 19 are written below.
  if (file.byteLength < 100) {
    throw new FileRefused(NOT_UPHEAP);
  }
  const { capi, wasm } = sqlite3;
  const db = new sqlite3.oo1.DB(":memory:");
  try {
    const pointer = wasm.allocFromTypedArray(file);
    // As `importDb` does, mark the copy as not in WAL mode, which an in-memory database can't open. In the wasm heap,
    // so the file isn't copied once more.
    const heap = wasm.heap8u();
    heap[Number(pointer) + 18] = 1;
    heap[Number(pointer) + 19] = 1;
    // FREEONCLOSE hands the memory to SQLite, which frees it when the database closes, or at once if this fails.
    const rc = capi.sqlite3_deserialize(
      db.pointer ?? 0,
      "main",
      pointer,
      file.byteLength,
      file.byteLength,
      capi.SQLITE_DESERIALIZE_FREEONCLOSE | capi.SQLITE_DESERIALIZE_READONLY,
    );
    if (rc !== capi.SQLITE_OK) {
      throw new FileRefused(NOT_UPHEAP);
    }
    checkUpheapDatabase(db);
    return countDatabase(db);
  } catch (error) {
    // Running out of memory for a large file, say, isn't the file's fault.
    throw error instanceof FileRefused ? error : new FileRefused(`Upheap couldn't read this file: ${String(error)}`);
  } finally {
    db.close();
  }
}
