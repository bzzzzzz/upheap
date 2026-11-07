//! The upheap server: one binary serving the API and the web app on one port.
//!
//! `upheap-server` runs the server. `upheap-server openapi` prints the OpenAPI document and exits.

mod assets;
mod config;

use std::sync::Arc;
use std::time::Duration as StdDuration;

use anyhow::Context;
use api::{AppState, Authentication, CookieSettings};
use config::{Config, LogFormat};
use service::{Services, Settings, SystemClock};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        println!("{}", serde_json::to_string_pretty(&api::openapi())?);
        return Ok(());
    }

    let config = Config::from_env().context("invalid configuration")?;
    init_tracing(config.log_format);
    if let Err(err) = run(config).await {
        tracing::error!(error = format!("{err:#}"), "server stopped");
        return Err(err);
    }
    Ok(())
}

async fn run(config: Config) -> anyhow::Result<()> {
    let db = store::connect(&config.database_url)
        .await
        .context("can't connect to the database")?;
    store::run_migrations(&db).await.context("migrations failed")?;

    let session_ttl = domain::Duration::from_days(i64::from(config.session_ttl_days));
    let services = Services::new(
        db,
        Arc::new(SystemClock),
        Settings {
            allow_registration: config.allow_registration,
            session_ttl,
            sign_in_limit: domain::sign_in::SignInLimit::DEFAULT,
        },
    );
    let cookies = CookieSettings {
        secure: config.cookies_secure(),
        max_age: StdDuration::from_secs(u64::from(config.session_ttl_days) * 24 * 60 * 60),
    };
    let app = api::router(AppState {
        services,
        authentication: Authentication::Sessions(cookies),
    })
    .fallback(assets::serve);

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("can't listen on {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, base_url = %config.base_url, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server failed")?;
    tracing::info!("shut down cleanly");
    Ok(())
}

fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match format {
        LogFormat::Json => builder.json().flatten_event(true).with_current_span(true).init(),
        LogFormat::Pretty => builder.init(),
    }
}

/// Resolves on SIGTERM or Ctrl-C. In-flight requests then drain before the server exits.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %err, "can't listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "can't listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown requested; draining in-flight requests");
}
