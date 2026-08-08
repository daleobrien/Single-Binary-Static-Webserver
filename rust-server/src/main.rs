/// Compile-time-guarded `eprintln!` — when `DISABLE_LOGGING` is true at
/// build time, all output is compiled out (zero runtime cost).
macro_rules! elog {
    ($($arg:tt)*) => {
        if !$crate::config::DISABLE_LOGGING {
            eprintln!($($arg)*);
        }
    };
}

mod config;
mod error;
mod handlers;
mod logging;
#[cfg(not(disable_http3))]
mod quic_worker;
mod response;
mod shutdown;
mod sockets;
mod startup;
mod tcp_worker;
mod tls_stream;
mod worker_config;

use std::time::Duration;

use config::{NUM_WORKERS, PORT, SHUTDOWN_TIMEOUT_SECS};
use worker_config::WorkerConfig;
use logging::init_logging;
use shutdown::wait_for_shutdown;

// ── Compile-time generated assets ──────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = PORT;
    let shutdown_timeout = Duration::from_secs(SHUTDOWN_TIMEOUT_SECS);

    let num_workers = NUM_WORKERS;

    // ── CLI: --help / -h ───────────────────────────────────────────
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        startup::print_help();
    }

    let summary_mode = std::env::args().any(|arg| arg == "--summary");
    startup::print_banner(port, num_workers, ALL_ASSETS.len(), summary_mode);
    startup::print_assets_table();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install ring crypto provider");

    let tls_config = build_tls_config();

    // ── Shutdown coordination ──────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // ── Logging strategy ───────────────────────────────────────────
    let (log_mode, log_handle) =
        init_logging(summary_mode, shutdown_rx.clone());

    // ── Spawn workers ───────────────────────────────────────────────
    let worker_cfg = WorkerConfig {
        num_workers,
        port,
        tls_config: tls_config.clone(),
        log_mode: log_mode.clone(),
        shutdown_rx,
    };

    let mut handles = tcp_worker::spawn_tcp_workers(worker_cfg.clone())?;
    #[cfg(not(disable_http3))]
    handles.extend(quic_worker::spawn_quic_workers(worker_cfg)?);
    // Track the logging background task so it is joined during shutdown.
    handles.push(log_handle);

    // ── Wait for shutdown signal ────────────────────────────────────
    wait_for_shutdown(shutdown_tx, handles, shutdown_timeout).await;

    Ok(())
}
