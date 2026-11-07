//! The local build: the backend compiled to WebAssembly, running in the browser on SQLite-wasm.
//!
//! Spike: proves that `store` and `service` run unchanged over a SeaORM proxy connection whose SQL is executed by
//! SQLite-wasm. The JS side is `globalThis.upheapSqlite`, with `query` and `execute` (see `spike/shim.mjs`).
//!
//! Everything here is wasm-only, so a native `cargo build --workspace` builds an empty crate.
#![cfg(target_arch = "wasm32")]

mod clock;
mod proxy;

use std::sync::Arc;

use domain::item::{NewItem, Size, Title};
use domain::list_view::ListView;
use domain::sign_in::SignInLimit;
use domain::{Duration, HeapId};
use sea_orm::{Database, DbBackend};
use service::auth::Registration;
use service::{Principal, Services, Settings};
use wasm_bindgen::prelude::*;

/// Opens the database through the proxy, migrates it, registers a user, adds three items and ranks them.
/// Returns what happened as JSON text, for the spike page to show.
#[wasm_bindgen]
pub async fn spike() -> Result<String, JsError> {
    std::panic::set_hook(Box::new(|info| proxy::report(&format!("panic: {info}"))));
    let proxy: Arc<Box<dyn sea_orm::ProxyDatabaseTrait>> = Arc::new(Box::new(proxy::SqliteWasm::new()));
    let db = Database::connect_proxy(DbBackend::Sqlite, proxy).await.map_err(to_js)?;
    let applied = store::run_migrations(&db).await.map_err(to_js)?;

    let services = Services::new(
        db.clone(),
        Arc::new(clock::BrowserClock),
        Settings {
            allow_registration: true,
            session_ttl: Duration::from_days(30),
            sign_in_limit: SignInLimit::DEFAULT,
        },
    );
    let email = format!("spike-{}@example.com", services.now().as_millis());
    let grant = services
        .register(Registration {
            email: &email,
            display_name: "Spike",
            password: "correct horse battery staple",
        })
        .await
        .map_err(to_js)?;
    let duplicate = services
        .register(Registration {
            email: &email,
            display_name: "Spike again",
            password: "correct horse battery staple",
        })
        .await
        .map(|_| "registered twice".to_owned())
        .unwrap_or_else(|err| format!("{err:?}"));
    let principal = Principal { user_id: grant.user.id };
    let me = services.load_me(principal).await.map_err(to_js)?;
    let heap_id = first_heap(&me).ok_or_else(|| JsError::new("the new user has no heap"))?;

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
        "duplicate_registration": duplicate,
        "transactions": check_transactions(&db).await.map_err(to_js)?,
        "user": email,
        "heap_id": heap_id.to_string(),
        "ranked": items,
    });
    Ok(report.to_string())
}

/// A dropped transaction leaves nothing behind, and a committed one keeps its row.
async fn check_transactions(db: &sea_orm::DatabaseConnection) -> Result<serde_json::Value, sea_orm::DbErr> {
    use sea_orm::{ConnectionTrait, TransactionTrait};
    db.execute_unprepared("CREATE TABLE IF NOT EXISTS spike_txn (n INTEGER)")
        .await?;
    db.execute_unprepared("DELETE FROM spike_txn").await?;
    {
        let txn = db.begin().await?;
        txn.execute_unprepared("INSERT INTO spike_txn VALUES (1)").await?;
        // Dropped without commit: SeaORM rolls it back.
    }
    let after_drop = count_rows(db).await?;
    let txn = db.begin().await?;
    txn.execute_unprepared("INSERT INTO spike_txn VALUES (2)").await?;
    txn.commit().await?;
    let after_commit = count_rows(db).await?;
    Ok(serde_json::json!({ "rows_after_drop": after_drop, "rows_after_commit": after_commit }))
}

async fn count_rows(db: &sea_orm::DatabaseConnection) -> Result<i32, sea_orm::DbErr> {
    use sea_orm::ConnectionTrait;
    let row = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            "SELECT COUNT(*) AS n FROM spike_txn",
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

/// Sends requests through the real axum Router: register, then read `/me` and the heap's ranked list with the
/// session cookie. Returns each response's status and the start of its body.
#[wasm_bindgen]
pub async fn spike_http() -> Result<String, JsError> {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    std::panic::set_hook(Box::new(|info| proxy::report(&format!("panic: {info}"))));
    let proxy: Arc<Box<dyn sea_orm::ProxyDatabaseTrait>> = Arc::new(Box::new(proxy::SqliteWasm::new()));
    let db = Database::connect_proxy(DbBackend::Sqlite, proxy).await.map_err(to_js)?;
    store::run_migrations(&db).await.map_err(to_js)?;
    let services = Services::new(
        db,
        Arc::new(clock::BrowserClock),
        Settings {
            allow_registration: true,
            session_ttl: Duration::from_days(30),
            sign_in_limit: SignInLimit::DEFAULT,
        },
    );
    let router = api::router(api::AppState {
        services,
        cookies: api::CookieSettings {
            secure: true,
            max_age: std::time::Duration::from_secs(30 * 24 * 60 * 60),
        },
    });

    let send = |request: Request<Body>| {
        let router = router.clone();
        async move {
            let response = router.oneshot(request).await.map_err(to_js)?;
            let status = response.status().as_u16();
            let cookie = response
                .headers()
                .get(header::SET_COOKIE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(';').next())
                .map(str::to_owned);
            let body = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .map_err(to_js)?;
            let body = String::from_utf8_lossy(&body).into_owned();
            Ok::<_, JsError>((status, cookie, body))
        }
    };

    let email = format!("http-{}@example.com", js_sys::Date::now() as i64);
    let register = Request::post("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-csrf-protection", "1")
        .body(Body::from(
            serde_json::json!({ "email": email, "display_name": "Http", "password": "correct horse battery staple" })
                .to_string(),
        ))
        .map_err(to_js)?;
    let (register_status, cookie, register_body) = send(register).await?;
    let cookie = cookie.unwrap_or_default();
    let me = Request::get("/api/v1/me")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .map_err(to_js)?;
    let (me_status, _, me_body) = send(me).await?;
    let heap_id = serde_json::from_str::<serde_json::Value>(&me_body)
        .ok()
        .and_then(|me| me["teams"][0]["heaps"][0]["id"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let list = Request::get(format!("/api/v1/heaps/{heap_id}/items"))
        .header(header::COOKIE, &cookie)
        .header("time-zone", "Europe/Berlin")
        .body(Body::empty())
        .map_err(to_js)?;
    let (list_status, _, list_body) = send(list).await?;
    let missing = Request::get("/api/v1/items/00000000-0000-0000-0000-000000000000")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .map_err(to_js)?;
    let (missing_status, _, missing_body) = send(missing).await?;

    let short = |s: &str| s.chars().take(160).collect::<String>();
    Ok(serde_json::json!({
        "register": [register_status, short(&register_body)],
        "me": [me_status, short(&me_body)],
        "ranked_list": [list_status, short(&list_body)],
        "missing_item": [missing_status, short(&missing_body)],
    })
    .to_string())
}
