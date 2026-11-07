// The SQLite side of the proxy connection: `globalThis.upheapSqlite`, called synchronously from wasm.
// `query` returns `{ columns, decltypes, rows }`; `execute` returns `{ changes, lastInsertRowid }`.
export function installShim(sqlite3, db) {
  const capi = sqlite3.capi;
  const withStatement = (sql, params, use) => {
    const stmt = db.prepare(sql);
    try {
      if (params.length > 0) stmt.bind(params);
      return use(stmt);
    } finally {
      stmt.finalize();
    }
  };
  globalThis.upheapSqlite = {
    query(sql, params) {
      return withStatement(sql, params, (stmt) => {
        const n = stmt.columnCount;
        const columns = [];
        const tables = [];
        const origins = [];
        const decltypes = [];
        for (let i = 0; i < n; i++) {
          columns.push(stmt.getColumnName(i));
          tables.push(capi.sqlite3_column_table_name(stmt.pointer, i));
          origins.push(capi.sqlite3_column_origin_name(stmt.pointer, i));
          decltypes.push(capi.sqlite3_column_decltype(stmt.pointer, i));
        }
        const rows = [];
        while (stmt.step()) rows.push(stmt.get([]));
        if (globalThis.UPHEAP_SQL_LOG) console.log("query", sql, JSON.stringify({ columns, decltypes, rows: rows.length, first: rows[0] }));
        return { columns, tables, origins, decltypes, rows };
      });
    },
    execute(sql, params) {
      withStatement(sql, params, (stmt) => {
        while (stmt.step()) {
          // Statements with RETURNING still have to be stepped to the end.
        }
      });
      return { changes: db.changes(), lastInsertRowid: Number(capi.sqlite3_last_insert_rowid(db.pointer)) };
    },
  };
}
