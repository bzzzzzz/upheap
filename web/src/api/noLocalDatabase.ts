// The server build's `#local-database`: none, since the data is on the server. The local build resolves it to
// src/local/database.ts instead (package.json `imports`, vite.config.ts).
import type { DatabaseCounts, LocalDatabase } from "../local/database.ts";

export type { DatabaseCounts, LocalDatabase };

export const localDatabase: LocalDatabase | null = null;
