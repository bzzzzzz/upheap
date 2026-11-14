//! The upheap server: one binary serving the API and the web app on one port.
//!
//! `upheap-server` runs the server. `upheap-server openapi` prints the OpenAPI document and exits.
//! `upheap-server user reset-link <username or email>` makes a password reset link ([`user_command`]).
//! `upheap-server mail test <address>` sends a test mail ([`mail_command`]).

mod assets;
mod config;
mod mail_command;
mod user_command;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use anyhow::Context;
use api::{AppState, CookieSettings};
use config::{Config, LogFormat};
use service::{Services, Settings, SystemClock};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// mimalloc instead of the system allocator. The Docker image links against musl, whose allocator serialises
/// threads on a lock and makes a busy multi-threaded server markedly slower.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("openapi") => {
            println!("{}", serde_json::to_string_pretty(&api::openapi())?);
            return Ok(ExitCode::SUCCESS);
        }
        Some("user") => return Ok(user_command::run(&args[1..]).await),
        Some("mail") => return Ok(mail_command::run(&args[1..]).await),
        _ => {}
    }

    let config = Config::from_env().context("invalid configuration")?;
    init_tracing(config.log_format, "info", BoxMakeWriter::new(std::io::stdout));
    if let Err(err) = run(config).await {
        tracing::error!(error = format!("{err:#}"), "server stopped");
        return Err(err);
    }
    Ok(ExitCode::SUCCESS)
}

/// The use cases with the deployment settings from `config`, on `db`.
fn build_services(config: &Config, db: store::DatabaseConnection) -> Services {
    Services::new(
        db,
        Arc::new(SystemClock),
        Settings {
            allow_registration: config.allow_registration,
            session_ttl: domain::Duration::from_days(i64::from(config.session_ttl_days)),
            sign_in_limit: domain::sign_in::SignInLimit::DEFAULT,
        },
    )
}

async fn run(config: Config) -> anyhow::Result<()> {
    let db = store::connect(&config.database_url, config.database_max_connections)
        .await
        .context("can't connect to the database")?;
    store::run_migrations(&db).await.context("migrations failed")?;

    let services = build_services(&config, db);
    let cookies = CookieSettings {
        secure: config.cookies_secure(),
        max_age: StdDuration::from_secs(u64::from(config.session_ttl_days) * 24 * 60 * 60),
    };
    let app = api::router(AppState { services, cookies }).fallback(assets::serve);

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("can't listen on {}", config.bind_addr))?;
    let mail = config.mail.as_ref().map_or("off", mail::Mailer::describe);
    tracing::info!(addr = %config.bind_addr, base_url = %config.base_url, mail, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server failed")?;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// Starts logging to `writer` at `RUST_LOG`, or at `default_level` when it isn't set.
fn init_tracing(format: LogFormat, default_level: &str, writer: BoxMakeWriter) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer);
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
