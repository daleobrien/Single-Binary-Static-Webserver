mod config;
mod error;
mod handlers;
mod logging;
mod quic_worker;
mod shutdown;
mod sockets;
mod tcp_worker;
mod tls_stream;

// HeaderMap is used by the compile-time generated code included below.
use hyper::HeaderMap;
use std::sync::LazyLock;
use std::time::Duration;

use config::{NUM_WORKERS, PORT, SHUTDOWN_TIMEOUT_SECS, WorkerConfig};
use logging::init_logging;
use shutdown::wait_for_shutdown;

// ── Compile-time generated assets ──────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = *PORT;
    let shutdown_timeout = Duration::from_secs(*SHUTDOWN_TIMEOUT_SECS);

    let num_workers = *NUM_WORKERS;

    // ── CLI: --help / -h ───────────────────────────────────────────
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        eprintln!("Usage: app [--summary]");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --summary    Log aggregated req/s every 5s instead of per-request details");
        eprintln!("  --help, -h   Show this help message");
        eprintln!();
        eprintln!("Environment variables:");
        eprintln!("  PORT                  Server port (default: 3000)");
        eprintln!("  WORKERS               Number of worker threads (default: available parallelism)");
        eprintln!("  SHUTDOWN_TIMEOUT_SECS Graceful shutdown timeout in seconds (default: 30)");
        return Ok(());
    }

    let summary_mode = std::env::args().any(|arg| arg == "--summary");

    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!("  http://localhost:{port}  |  https://localhost:{port}");
    eprintln!();
    eprintln!(
        "  {} assets  ·  {num_workers} workers  ·  SO_REUSEPORT  ·  h1.1 / h2 / h3",
        ALL_ASSETS.len()
    );
    if summary_mode {
        eprintln!("  Log: req/s reported every 5s");
    }
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    LazyLock::force(&HEADER_MAPS);

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install ring crypto provider");

    let tls_config = build_tls_config();

    // ── Shutdown coordination ──────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // ── Logging strategy ───────────────────────────────────────────
    let (log_mode, log_handle) =
        init_logging(summary_mode, MAX_PATH_LEN, MAX_SIZE_DIGITS, shutdown_rx.clone());

    // ── Spawn workers ───────────────────────────────────────────────
    let worker_cfg = WorkerConfig {
        num_workers,
        port,
        tls_config: tls_config.clone(),
        log_mode: log_mode.clone(),
        shutdown_rx,
    };

    let mut handles = tcp_worker::spawn_tcp_workers(worker_cfg.clone())?;
    handles.extend(quic_worker::spawn_quic_workers(worker_cfg)?);
    // Track the logging background task so it is joined during shutdown.
    handles.push(log_handle);

    // ── Wait for shutdown signal ────────────────────────────────────
    wait_for_shutdown(shutdown_tx, handles, shutdown_timeout).await;

    Ok(())
}
