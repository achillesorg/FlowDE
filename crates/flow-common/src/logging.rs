//! Logging via `tracing`. Under systemd (JOURNAL_STREAM set) timestamps are
//! dropped because the journal adds them.

use tracing_subscriber::EnvFilter;

/// Initialise logging; level from `FLOW_LOG` or `RUST_LOG`, default `info`.
/// Safe to call more than once (later calls are no-ops).
pub fn init(daemon: &str) {
    let filter = std::env::var("FLOW_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .map(EnvFilter::new)
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let b = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::io::stderr);
    let r = if std::env::var_os("JOURNAL_STREAM").is_some() {
        b.without_time().try_init()
    } else {
        b.try_init()
    };
    if r.is_ok() {
        tracing::info!("{daemon} starting");
    }
}
