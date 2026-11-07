// The backend's worker: SQLite-wasm on OPFS (`opfs-sahpool`, which needs no COOP/COEP headers) and the wasm.
import sqlite3InitModule from "./node_modules/@sqlite.org/sqlite-wasm/dist/index.mjs";
import { installShim } from "./shim.mjs";
import init, { spike, spike_http } from "../pkg/local.js";

const started = performance.now();
const sqlite3 = await sqlite3InitModule();
const pool = await sqlite3.installOpfsSAHPoolVfs({ name: "upheap-spike" });
const db = new pool.OpfsSAHPoolDb("/upheap.sqlite3");
installShim(sqlite3, db);
await init();
postMessage({ ready: true, vfs: "opfs-sahpool", startup_ms: Math.round(performance.now() - started) });

self.onmessage = async () => {
  const t = performance.now();
  try {
    const ok = JSON.parse(await spike());
    const spike_ms = Math.round(performance.now() - t);
    const t2 = performance.now();
    const http = JSON.parse(await spike_http());
    postMessage({ ok, spike_ms, http: Object.fromEntries(Object.entries(http).map(([k, v]) => [k, v[0]])), http_ms: Math.round(performance.now() - t2) });
  } catch (e) {
    postMessage({ error: String(e) });
  }
};
