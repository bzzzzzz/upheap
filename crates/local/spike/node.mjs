// Runs the spike headless: in-memory SQLite-wasm in Node, the backend wasm from `pkg/`.
import { readFile } from "node:fs/promises";
import sqlite3InitModule from "@sqlite.org/sqlite-wasm";
import { installShim } from "./shim.mjs";
import init, { spike, spike_http } from "../pkg/local.js";

const sqlite3 = await sqlite3InitModule();
const db = new sqlite3.oo1.DB(":memory:");
installShim(sqlite3, db);
await init({ module_or_path: await readFile(new URL("../pkg/local_bg.wasm", import.meta.url)) });
console.log(JSON.stringify(JSON.parse(await spike()), null, 2));
console.log(JSON.stringify(JSON.parse(await spike_http()), null, 2));
