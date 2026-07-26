mod config;
mod error;
mod handlers;
mod logging;
mod quic_worker;
mod shutdown;
mod sockets;
mod startup;
mod tcp_worker;
mod tls_stream;

use hyper::HeaderMap;
use std::sync::LazyLock;
use std::time::Duration;

use config::{PORT, SHUTDOWN_TIMEOUT_SECS, WorkerConfig};
use logging::init_logging;
use shutdown::wait_for_shutdown;

// ── Compile-time generated assets ──────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = *PORT;
    let shutdown_timeout = Duration::from_secs(*SHUTDOWN_TIMEOUT_SECS);

    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let summary_mode = std::env::args().any(|arg| arg == "--summary");

    startup::print_banner(port, num_workers, ALL_ASSETS.len(), summary_mode);

    LazyLock::force(&HEADER_MAPS);

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install ring crypto provider");

    let tls_config = build_tls_config();

    // ── Shutdown coordination ──────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // ── Logging strategy ───────────────────────────────────────────
    let log_mode = init_logging(summary_mode, MAX_PATH_LEN, MAX_SIZE_DIGITS);

    // ── Spawn workers ───────────────────────────────────────────────
    let worker_cfg = WorkerConfig {
        num_workers,
        port,
        tls_config: tls_config.clone(),
        log_mode: log_mode.clone(),
        shutdown_rx: shutdown_rx.clone(),
    };

    let mut handles = tcp_worker::spawn_tcp_workers(worker_cfg.clone())?;
    handles.extend(quic_worker::spawn_quic_workers(worker_cfg)?);

    // ── Wait for shutdown signal ────────────────────────────────────
    wait_for_shutdown(shutdown_tx, handles, shutdown_timeout).await;

    Ok(())
}
