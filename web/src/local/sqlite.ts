// The SQLite side of the local build's database connection: `globalThis.upheapSqlite`, which the backend's proxy
// (crates/local/src/proxy.rs) calls synchronously with each statement's SQL and parameters.
import type { Database, PreparedStatement, SqlValue, Sqlite3Static } from "@sqlite.org/sqlite-wasm";

/** A query's result: each column's name, the table and column it came from, its declared type, and the rows. */
type QueryResult = {
  columns: string[];
  tables: (string | null)[];
  origins: (string | null)[];
  decltypes: (string | null)[];
  rows: SqlValue[][];
};

type ExecuteResult = { changes: number; lastInsertRowid: number };

/** The connection the backend uses, on `globalThis`. A failed statement throws SQLite's error, with its code. */
export type UpheapSqlite = {
  query(sql: string, params: SqlValue[]): QueryResult;
  execute(sql: string, params: SqlValue[]): ExecuteResult;
};

/** The column metadata functions, which the build has but its types leave out. */
type ColumnMetadata = {
  sqlite3_column_table_name(stmt: number, column: number): string | null;
  sqlite3_column_origin_name(stmt: number, column: number): string | null;
  sqlite3_column_decltype(stmt: number, column: number): string | null;
};

declare global {
  var upheapSqlite: UpheapSqlite | undefined;
}

/** Points the backend at `db`. Call it before `Backend.open()`. */
export function installSqlite(sqlite3: Sqlite3Static, db: Database): void {
  const capi = sqlite3.capi as unknown as ColumnMetadata;
  const withStatement = <T>(sql: string, params: SqlValue[], read: (stmt: PreparedStatement) => T): T => {
    const stmt = db.prepare(sql);
    try {
      if (params.length > 0) {
        stmt.bind(params);
      }
      return read(stmt);
    } finally {
      stmt.finalize();
    }
  };

  globalThis.upheapSqlite = {
    query(sql, params) {
      return withStatement(sql, params, (stmt) => {
        const pointer = stmt.pointer ?? 0;
        const result: QueryResult = { columns: [], tables: [], origins: [], decltypes: [], rows: [] };
        for (let i = 0; i < stmt.columnCount; i++) {
          result.columns.push(stmt.getColumnName(i));
          result.tables.push(capi.sqlite3_column_table_name(pointer, i));
          result.origins.push(capi.sqlite3_column_origin_name(pointer, i));
          result.decltypes.push(capi.sqlite3_column_decltype(pointer, i));
        }
        while (stmt.step()) {
          result.rows.push(stmt.get([]));
        }
        return result;
      });
    },
    execute(sql, params) {
      withStatement(sql, params, (stmt) => {
        // A statement with RETURNING still has to be stepped to the end.
        while (stmt.step()) {
          /* nothing to read */
        }
      });
      return {
        changes: Number(db.changes()),
        lastInsertRowid: Number(sqlite3.capi.sqlite3_last_insert_rowid(db)),
      };
    },
  };
}
