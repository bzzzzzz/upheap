//! A SeaORM proxy connection whose SQL runs in SQLite-wasm, through `globalThis.upheapSqlite`.
//!
//! SeaORM's proxy rows must hold exactly the `Value` variant the entity field expects (an `i32` field needs
//! `Value::Int`, not `Value::BigInt`). SQLite declares both as `integer`, so each result column is typed from the
//! entity column it comes from (`store::proxy`), found through SQLite's column metadata. Columns of tables without
//! an entity (SeaORM's migration table) are typed from their declared type, and expressions from their value.
//!
//! The JS calls are synchronous and no JS value is held across an `await`, so the futures are `Send` as SeaORM
//! requires, although the worker has only one thread.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use js_sys::{Array, Reflect, Uint8Array};
use sea_orm::{ColumnType, DbErr, ProxyDatabaseTrait, ProxyExecResult, ProxyRow, RuntimeErr, Statement, Value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Runs a statement that returns rows: `{ columns, tables, origins, decltypes, rows }`, where `tables` and
    /// `origins` name the table column each result column comes from, or are `null` for an expression.
    #[wasm_bindgen(js_namespace = upheapSqlite, js_name = query, catch)]
    fn js_query(sql: &str, params: Array) -> Result<JsValue, JsValue>;

    /// Runs a statement that changes rows: `{ changes, lastInsertRowid }`.
    #[wasm_bindgen(js_namespace = upheapSqlite, js_name = execute, catch)]
    fn js_execute(sql: &str, params: Array) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

thread_local! {
    /// Set when `COMMIT` fails. SeaORM's proxy hooks can't return errors, so the caller checks this after the
    /// use case and fails the request.
    static COMMIT_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Writes to the browser console as an error.
pub fn report(message: &str) {
    console_error(message);
}

/// Takes the error of a failed `COMMIT` since the last call, if there was one.
pub fn take_commit_error() -> Option<String> {
    COMMIT_ERROR.with(|cell| cell.borrow_mut().take())
}

#[derive(Debug)]
pub struct SqliteWasm {
    /// The kind of every entity column, by table and column name.
    columns: HashMap<(&'static str, &'static str), ColumnKind>,
}

impl SqliteWasm {
    pub fn new() -> Self {
        let columns = store::proxy::list_column_specs()
            .into_iter()
            .filter_map(|spec| Some(((spec.table, spec.column), entity_column_kind(&spec.column_type)?)))
            .collect();
        Self { columns }
    }

    fn column_kind(&self, table: Option<String>, origin: Option<String>, decltype: Option<String>) -> ColumnKind {
        let from_entity = table
            .zip(origin)
            .and_then(|(table, origin)| self.columns.get(&(table.as_str(), origin.as_str())).copied());
        from_entity.unwrap_or_else(|| declared_column_kind(decltype.as_deref()))
    }
}

#[async_trait::async_trait]
impl ProxyDatabaseTrait for SqliteWasm {
    async fn query(&self, statement: Statement) -> Result<Vec<ProxyRow>, DbErr> {
        let result = js_query(&statement.sql, to_js_params(&statement)?).map_err(js_err)?;
        self.to_rows(&result)
    }

    async fn execute(&self, statement: Statement) -> Result<ProxyExecResult, DbErr> {
        let result = js_execute(&statement.sql, to_js_params(&statement)?).map_err(js_err)?;
        let changes = get_number(&result, "changes")?;
        let last_insert_id = get_number(&result, "lastInsertRowid")?;
        Ok(ProxyExecResult {
            last_insert_id: last_insert_id as u64,
            rows_affected: changes as u64,
        })
    }

    async fn begin(&self) {
        run_control("BEGIN IMMEDIATE");
    }

    async fn commit(&self) {
        if let Err(err) = js_execute("COMMIT", Array::new()) {
            let message = describe(&err);
            console_error(&format!("COMMIT failed, rolling back: {message}"));
            run_control("ROLLBACK");
            COMMIT_ERROR.with(|cell| *cell.borrow_mut() = Some(message));
        }
    }

    async fn rollback(&self) {
        run_control("ROLLBACK");
    }

    fn start_rollback(&self) {
        run_control("ROLLBACK");
    }
}

fn run_control(sql: &str) {
    if let Err(err) = js_execute(sql, Array::new()) {
        console_error(&format!("{sql} failed: {}", describe(&err)));
    }
}

fn to_js_params(statement: &Statement) -> Result<Array, DbErr> {
    let params = Array::new();
    let Some(values) = &statement.values else {
        return Ok(params);
    };
    for value in values.iter() {
        params.push(&to_js_value(value)?);
    }
    Ok(params)
}

fn to_js_value(value: &Value) -> Result<JsValue, DbErr> {
    let js = match value {
        Value::Bool(v) => v.map(|b| JsValue::from(i32::from(b))),
        Value::TinyInt(v) => v.map(JsValue::from),
        Value::SmallInt(v) => v.map(JsValue::from),
        Value::Int(v) => v.map(JsValue::from),
        Value::BigInt(v) => v.map(|n| JsValue::from(n as f64)),
        Value::TinyUnsigned(v) => v.map(JsValue::from),
        Value::SmallUnsigned(v) => v.map(JsValue::from),
        Value::Unsigned(v) => v.map(JsValue::from),
        Value::BigUnsigned(v) => v.map(|n| JsValue::from(n as f64)),
        Value::Float(v) => v.map(JsValue::from),
        Value::Double(v) => v.map(JsValue::from),
        Value::String(v) => v.as_deref().map(JsValue::from_str),
        Value::Char(v) => v.map(|c| JsValue::from_str(c.encode_utf8(&mut [0; 4]))),
        Value::Bytes(v) => v.as_deref().map(|b| Uint8Array::from(b).into()),
        Value::Json(v) => v.as_deref().map(|j| JsValue::from_str(&j.to_string())),
        other => return Err(DbErr::Custom(format!("unsupported parameter type: {other:?}"))),
    };
    Ok(js.unwrap_or(JsValue::NULL))
}

/// What a column holds, from its declared type.
#[derive(Clone, Copy, Debug)]
enum ColumnKind {
    Bool,
    Int,
    BigInt,
    Double,
    Text,
    Blob,
    /// No declared type: an expression. Typed from the value.
    Unknown,
}

/// The kind an entity column is read as, or `None` when only its declared type can tell.
fn entity_column_kind(column_type: &ColumnType) -> Option<ColumnKind> {
    let kind = match column_type {
        ColumnType::Boolean => ColumnKind::Bool,
        ColumnType::TinyInteger | ColumnType::SmallInteger | ColumnType::Integer => ColumnKind::Int,
        ColumnType::BigInteger => ColumnKind::BigInt,
        ColumnType::Float | ColumnType::Double | ColumnType::Decimal(_) => ColumnKind::Double,
        ColumnType::Char(_) | ColumnType::String(_) | ColumnType::Text => ColumnKind::Text,
        ColumnType::Binary(_) | ColumnType::VarBinary(_) | ColumnType::Blob => ColumnKind::Blob,
        _ => return None,
    };
    Some(kind)
}

/// The same affinity rules as SQLite's, with booleans and big integers told apart as sea-query declares them.
fn declared_column_kind(decltype: Option<&str>) -> ColumnKind {
    let Some(decl) = decltype else {
        return ColumnKind::Unknown;
    };
    let decl = decl.to_ascii_lowercase();
    if decl.contains("bool") {
        ColumnKind::Bool
    } else if decl.contains("bigint") || decl.contains("int8") {
        ColumnKind::BigInt
    } else if decl.contains("int") {
        ColumnKind::Int
    } else if decl.contains("char") || decl.contains("text") || decl.contains("clob") || decl.contains("json") {
        ColumnKind::Text
    } else if decl.contains("blob") || decl.contains("binary") {
        ColumnKind::Blob
    } else if decl.contains("real") || decl.contains("floa") || decl.contains("doub") || decl.contains("decimal") {
        ColumnKind::Double
    } else {
        ColumnKind::Unknown
    }
}

impl SqliteWasm {
    fn to_rows(&self, result: &JsValue) -> Result<Vec<ProxyRow>, DbErr> {
        let strings = |key| -> Result<Vec<Option<String>>, DbErr> {
            Ok(get_array(result, key)?.iter().map(|v| v.as_string()).collect())
        };
        let columns: Vec<String> = strings("columns")?.into_iter().map(Option::unwrap_or_default).collect();
        let kinds: Vec<ColumnKind> = strings("tables")?
            .into_iter()
            .zip(strings("origins")?)
            .zip(strings("decltypes")?)
            .map(|((table, origin), decltype)| self.column_kind(table, origin, decltype))
            .collect();
        to_rows(result, &columns, &kinds)
    }
}

fn to_rows(result: &JsValue, columns: &[String], kinds: &[ColumnKind]) -> Result<Vec<ProxyRow>, DbErr> {
    get_array(result, "rows")?
        .iter()
        .map(|row| {
            let row = Array::from(&row);
            let values = columns
                .iter()
                .zip(kinds)
                .enumerate()
                .map(|(i, (name, kind))| Ok((name.clone(), to_value(*kind, &row.get(i as u32))?)))
                .collect::<Result<BTreeMap<_, _>, DbErr>>()?;
            Ok(ProxyRow { values })
        })
        .collect()
}

fn to_value(kind: ColumnKind, js: &JsValue) -> Result<Value, DbErr> {
    let null = js.is_null() || js.is_undefined();
    let number = || js.as_f64().or_else(|| js.is_bigint().then(|| bigint_to_f64(js)));
    let value = match kind {
        ColumnKind::Bool => Value::Bool((!null).then(|| number().unwrap_or(0.0) != 0.0)),
        ColumnKind::Int => Value::Int((!null).then(|| number().unwrap_or(0.0) as i32)),
        ColumnKind::BigInt => Value::BigInt((!null).then(|| bigint_or_number(js))),
        ColumnKind::Double => Value::Double((!null).then(|| number().unwrap_or(0.0))),
        ColumnKind::Text => Value::String(js.as_string()),
        ColumnKind::Blob => Value::Bytes((!null).then(|| Uint8Array::new(js).to_vec())),
        ColumnKind::Unknown if null => Value::Int(None),
        ColumnKind::Unknown if js.is_string() => Value::String(js.as_string()),
        ColumnKind::Unknown if js.is_bigint() => Value::BigInt(Some(bigint_or_number(js))),
        ColumnKind::Unknown => match js.as_f64() {
            Some(n) if n.fract() == 0.0 && n.abs() <= f64::from(i32::MAX) => Value::Int(Some(n as i32)),
            Some(n) if n.fract() == 0.0 => Value::BigInt(Some(n as i64)),
            Some(n) => Value::Double(Some(n)),
            None => Value::Bytes(Some(Uint8Array::new(js).to_vec())),
        },
    };
    Ok(value)
}

fn bigint_or_number(js: &JsValue) -> i64 {
    if js.is_bigint() {
        i64::try_from(js.clone()).unwrap_or_default()
    } else {
        js.as_f64().unwrap_or(0.0) as i64
    }
}

fn bigint_to_f64(js: &JsValue) -> f64 {
    bigint_or_number(js) as f64
}

fn get_array(object: &JsValue, key: &str) -> Result<Array, DbErr> {
    let value = Reflect::get(object, &JsValue::from_str(key)).map_err(js_err)?;
    if !Array::is_array(&value) {
        return Err(DbErr::Custom(format!("SQLite shim returned no `{key}` array")));
    }
    Ok(Array::from(&value))
}

fn get_number(object: &JsValue, key: &str) -> Result<f64, DbErr> {
    let value = Reflect::get(object, &JsValue::from_str(key)).map_err(js_err)?;
    value
        .as_f64()
        .or_else(|| value.is_bigint().then(|| bigint_to_f64(&value)))
        .ok_or_else(|| DbErr::Custom(format!("SQLite shim returned no `{key}` number")))
}

/// SQLite's extended result codes for a broken unique key and a broken primary key.
const SQLITE_CONSTRAINT_UNIQUE: f64 = 2067.0;
const SQLITE_CONSTRAINT_PRIMARYKEY: f64 = 1555.0;

fn js_err(err: JsValue) -> DbErr {
    let message = describe(&err);
    let code = Reflect::get(&err, &JsValue::from_str("resultCode"))
        .ok()
        .and_then(|c| c.as_f64());
    if matches!(code, Some(SQLITE_CONSTRAINT_UNIQUE | SQLITE_CONSTRAINT_PRIMARYKEY)) {
        return store::proxy::unique_violation(&message);
    }
    DbErr::Query(RuntimeErr::Internal(message))
}

fn describe(err: &JsValue) -> String {
    Reflect::get(err, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .or_else(|| err.as_string())
        .unwrap_or_else(|| format!("{err:?}"))
}
