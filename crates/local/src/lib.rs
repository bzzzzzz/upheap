//! The local build: the backend compiled to WebAssembly, running in the browser on SQLite-wasm.
//!
//! Spike: proves that `store` and `service` run unchanged over a SeaORM proxy connection whose SQL is executed by
//! SQLite-wasm. The JS side is `globalThis.upheapSqlite`, with `query` and `execute` (see `spike/shim.mjs`).
//! `service` and `api` are built without accounts, so there is one user, made on first start.
//!
//! Everything here is wasm-only, so a native `cargo build --workspace` builds an empty crate.
#![cfg(target_arch = "wasm32")]

mod clock;
mod proxy;

use std::sync::Arc;

use domain::HeapId;
use domain::item::{NewItem, Size, Title};
use domain::list_view::ListView;
use sea_orm::{Database, DbBackend};
use service::{Services, Settings};
use wasm_bindgen::prelude::*;

/// Opens the database through the proxy, migrates it, opens the local user, adds three items and ranks them.
/// Returns what happened as JSON text, for the spike page to show.
#[wasm_bindgen]
pub async fn spike() -> Result<String, JsError> {
    std::panic::set_hook(Box::new(|info| proxy::report(&format!("panic: {info}"))));
    let proxy: Arc<Box<dyn sea_orm::ProxyDatabaseTrait>> = Arc::new(Box::new(proxy::SqliteWasm::new()));
    let db = Database::connect_proxy(DbBackend::Sqlite, proxy).await.map_err(to_js)?;
    let applied = store::run_migrations(&db).await.map_err(to_js)?;

    let services = Services::new(db.clone(), Arc::new(clock::BrowserClock), Settings::default());
    let principal = services.open_local_user().await.map_err(to_js)?;
    let reopened = services.open_local_user().await.map_err(to_js)?;
    let me = services.load_me(principal).await.map_err(to_js)?;
    let heap_id = first_heap(&me).ok_or_else(|| JsError::new("the local user has no heap"))?;

    for (title, effort, impact) in [
        ("Small and valuable", Size::S, Size::Xl),
        ("Big and dull", Size::Xl, Size::S),
        ("Middling", Size::M, Size::M),
    ] {
        let fields = NewItem {
            title: Title::parse(title).map_err(to_js)?,
            description: None,
            effort: Some(effort),
            impact: Some(impact),
            due: None,
            tags: Default::default(),
            parameters: Default::default(),
        };
        services.create_item(principal, heap_id, fields).await.map_err(to_js)?;
    }
    let ranked = services
        .list_ranked(principal, heap_id, None, &ListView::default())
        .await
        .map_err(to_js)?;

    if let Some(err) = proxy::take_commit_error() {
        return Err(JsError::new(&format!("a commit failed: {err}")));
    }

    let items: Vec<_> = ranked
        .ranked
        .iter()
        .map(|entry| {
            serde_json::json!({
                "position": entry.position,
                "title": entry.view.item.title.as_str(),
                "contributors": entry.contributors.iter().map(|c| &c.label).collect::<Vec<_>>(),
            })
        })
        .collect();
    let report = serde_json::json!({
        "migrations_applied": applied,
        "same_user_reopened": reopened == principal,
        "transactions": check_transactions(&db).await.map_err(to_js)?,
        "user": principal.user_id.to_string(),
        "heap_id": heap_id.to_string(),
        "ranked": items,
    });
    Ok(report.to_string())
}

/// A dropped transaction leaves nothing behind, a committed one keeps its row, and a duplicate key is a
/// [`store::StoreError::Duplicate`].
async fn check_transactions(db: &sea_orm::DatabaseConnection) -> Result<serde_json::Value, sea_orm::DbErr> {
    use sea_orm::{ConnectionTrait, TransactionTrait};
    db.execute_unprepared("CREATE TABLE IF NOT EXISTS spike_unique (n INTEGER UNIQUE)")
        .await?;
    db.execute_unprepared("DELETE FROM spike_unique").await?;
    {
        let txn = db.begin().await?;
        txn.execute_unprepared("INSERT INTO spike_unique VALUES (1)").await?;
        // Dropped without commit: SeaORM rolls it back.
    }
    let after_drop = count_rows(db).await?;
    let txn = db.begin().await?;
    txn.execute_unprepared("INSERT INTO spike_unique VALUES (2)").await?;
    txn.commit().await?;
    let after_commit = count_rows(db).await?;
    let duplicate = match db.execute_unprepared("INSERT INTO spike_unique VALUES (2)").await {
        Ok(_) => "inserted twice".to_owned(),
        Err(err) => format!("{:?}", store::StoreError::from(err)),
    };
    Ok(serde_json::json!({
        "rows_after_drop": after_drop,
        "rows_after_commit": after_commit,
        "duplicate_key": duplicate,
    }))
}

async fn count_rows(db: &sea_orm::DatabaseConnection) -> Result<i32, sea_orm::DbErr> {
    use sea_orm::ConnectionTrait;
    let row = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            "SELECT COUNT(*) AS n FROM spike_unique",
        ))
        .await?;
    row.map_or(Ok(0), |row| row.try_get("", "n"))
}

fn first_heap(me: &service::me::MeView) -> Option<HeapId> {
    me.teams.first()?.heaps.first().map(|heap| heap.id)
}

/// The error and every source under it, since a store error's own message says only which layer failed.
fn to_js(err: impl std::error::Error) -> JsError {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    JsError::new(&message)
}

/// Sends requests through the real axum Router as the local user: `/me`, a new item, the heap's ranked list and an
/// unknown item. Returns each response's status and the start of its body.
#[wasm_bindgen]
pub async fn spike_http() -> Result<String, JsError> {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    std::panic::set_hook(Box::new(|info| proxy::report(&format!("panic: {info}"))));
    let proxy: Arc<Box<dyn sea_orm::ProxyDatabaseTrait>> = Arc::new(Box::new(proxy::SqliteWasm::new()));
    let db = Database::connect_proxy(DbBackend::Sqlite, proxy).await.map_err(to_js)?;
    store::run_migrations(&db).await.map_err(to_js)?;
    let services = Services::new(db, Arc::new(clock::BrowserClock), Settings::default());
    let local_user = services.open_local_user().await.map_err(to_js)?;
    let router = api::router(api::AppState {
        services,
        authentication: api::Authentication::LocalUser(local_user),
    });

    let send = |request: Request<Body>| {
        let router = router.clone();
        async move {
            let response = router.oneshot(request).await.map_err(to_js)?;
            let status = response.status().as_u16();
            let body = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .map_err(to_js)?;
            Ok::<_, JsError>((status, String::from_utf8_lossy(&body).into_owned()))
        }
    };

    let me = Request::get("/api/v1/me").body(Body::empty()).map_err(to_js)?;
    let (me_status, me_body) = send(me).await?;
    let heap_id = serde_json::from_str::<serde_json::Value>(&me_body)
        .ok()
        .and_then(|me| me["teams"][0]["heaps"][0]["id"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let create = Request::post(format!("/api/v1/heaps/{heap_id}/items"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-csrf-protection", "1")
        .body(Body::from(
            serde_json::json!({ "title": "Over HTTP", "effort": "s", "impact": "l" }).to_string(),
        ))
        .map_err(to_js)?;
    let (create_status, create_body) = send(create).await?;
    let list = Request::get(format!("/api/v1/heaps/{heap_id}/items"))
        .header("time-zone", "Europe/Berlin")
        .body(Body::empty())
        .map_err(to_js)?;
    let (list_status, list_body) = send(list).await?;
    let missing = Request::get("/api/v1/items/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .map_err(to_js)?;
    let (missing_status, missing_body) = send(missing).await?;

    let short = |s: &str| s.chars().take(160).collect::<String>();
    Ok(serde_json::json!({
        "me": [me_status, short(&me_body)],
        "create_item": [create_status, short(&create_body)],
        "ranked_list": [list_status, short(&list_body)],
        "missing_item": [missing_status, short(&missing_body)],
    })
    .to_string())
}
