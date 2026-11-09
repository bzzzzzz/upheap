//! The `upheap` binary: gathers the environment once and runs the command.

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let deps = cli::Deps::system();
    init_logging(&deps.env);
    cli::run(std::env::args_os(), deps).await
}

/// Logs to `upheap.log` in the config directory, at the level `RUST_LOG` sets, when it's set: the terminal is for
/// results and errors, and a log is for finding out why something failed.
fn init_logging(env: &cli::Environment) {
    let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() else {
        return;
    };
    let Some(dir) = &env.config_dir else {
        return;
    };
    let opened = std::fs::create_dir_all(dir).and_then(|()| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("upheap.log"))
    });
    let Ok(file) = opened else {
        return;
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(file))
        .init();
}
