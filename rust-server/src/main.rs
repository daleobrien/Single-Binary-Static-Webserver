mod config;
mod error;
mod handlers;
mod logging;
mod quic_worker;
mod sockets;
mod tcp_worker;
mod tls_stream;

use hyper::HeaderMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use config::{PORT, SHUTDOWN_TIMEOUT_SECS};
use logging::LogMode;
use tokio_rustls::TlsAcceptor;

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

    eprintln!("Server running at http://localhost:{port}/  and  https://localhost:{port}/");
    eprintln!("All static files pre-compressed and embedded at compile time.");
    eprintln!(
        "{} assets baked into the binary (routing + header builders + bodies).",
        ALL_ASSETS.len()
    );
    eprintln!(
        "Starting {num_workers} workers with SO_REUSEPORT + auto (plain HTTP, TLS h1.1/h2/h3)"
    );
    if summary_mode {
        eprintln!("Log mode: summary (req/s every 5s)");
    }

    LazyLock::force(&HEADER_MAPS);

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install ring crypto provider");

    let tls_config = build_tls_config();
    let tls_acceptor = TlsAcceptor::from(Arc::clone(&tls_config));

    // ── Shutdown coordination ──────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // ── Logging strategy ───────────────────────────────────────────
    let log_mode = if summary_mode {
        let counter = Arc::new(AtomicU64::new(0));
        let counter_bg = Arc::clone(&counter);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await;
            loop {
                interval.tick().await;
                let count = counter_bg.swap(0, Ordering::Relaxed);
                eprintln!(
                    "{count} requests in the last 5s ({:.1} req/s)",
                    count as f64 / 5.0
                );
            }
        });

        LogMode::Summary(counter)
    } else {
        let path_w = MAX_PATH_LEN.max(1);
        let size_w = MAX_SIZE_DIGITS.max(1);
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<(String, String, u16, u64, u64, String)>();

        tokio::spawn(async move {
            eprintln!(
                "{:>2}  {:<7}  {:<path_w$}  {:>3}  {:>size_w$}  TIME",
                "PR",
                "METHOD",
                "PATH",
                "STA",
                "SIZE",
                path_w = path_w,
                size_w = size_w,
            );
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.tick().await;
            loop {
                interval.tick().await;

                let mut batch: Vec<(String, String, u16, u64, u64, String)> = Vec::new();
                while let Ok(entry) = rx.try_recv() {
                    batch.push(entry);
                }

                for (method, path, status, size, us, protocol) in &batch {
                    eprintln!(
                        "{protocol:>2}  {method:<7}  {path:<path_w$}  {status:>3}  {size:>size_w$}B  {us}\u{00b5}s",
                        path_w = path_w,
                        size_w = size_w,
                    );
                }
            }
        });

        LogMode::Detailed { tx, path_w, size_w }
    };

    // ── Spawn workers ───────────────────────────────────────────────
    let mut handles = tcp_worker::spawn_tcp_workers(
        num_workers,
        port,
        tls_acceptor,
        log_mode.clone(),
        shutdown_rx.clone(),
    )?;

    handles.extend(quic_worker::spawn_quic_workers(
        num_workers,
        port,
        tls_config,
        log_mode,
        shutdown_rx,
    ));

    // ── Wait for shutdown signal ────────────────────────────────────
    tokio::signal::ctrl_c().await.ok();
    eprintln!(
        "\nReceived shutdown signal — draining in-flight requests (timeout: {}s)...",
        shutdown_timeout.as_secs()
    );

    let _ = shutdown_tx.send(true);
    drop(shutdown_tx);

    let drain_future = async {
        for handle in handles {
            let _ = handle.await;
        }
    };

    match tokio::time::timeout(shutdown_timeout, drain_future).await {
        Ok(()) => eprintln!("Shutdown complete — all workers exited cleanly."),
        Err(_elapsed) => {
            eprintln!(
                "Shutdown timed out after {}s — forcing exit (some connections may have been dropped).",
                shutdown_timeout.as_secs()
            );
        }
    }

    Ok(())
}
