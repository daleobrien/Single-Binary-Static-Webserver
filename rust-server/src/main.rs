mod config;
mod error;
mod handlers;
mod logging;
mod sockets;
mod tls_stream;

use hyper::body::Incoming;
use hyper::{HeaderMap, Request};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper::service::service_fn;
use h3_quinn::Connection as H3QuinnConnection;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio_rustls::TlsAcceptor;

use config::{PORT, SHUTDOWN_TIMEOUT_SECS, TLS_CONTENT_TYPE_HANDSHAKE};
use logging::LogMode;
use sockets::{create_reuseport_listener, create_reuseport_udp_socket};
use tls_stream::PrefixedStream;
use handlers::{handle_request, handle_h3_connection};

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
    // Workers select between accepting new connections and this signal.
    // When the main task receives Ctrl+C, it sends `true` then drops
    // the sender; every worker immediately breaks its accept loop.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // ── Logging strategy ──────────────────────────────────────────────
    let log_mode = if summary_mode {
        let counter = Arc::new(AtomicU64::new(0));
        let counter_bg = Arc::clone(&counter);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await;
            loop {
                interval.tick().await;
                let count = counter_bg.swap(0, Ordering::Relaxed);
                eprintln!("{count} requests in the last 5s ({:.1} req/s)", count as f64 / 5.0);
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
                "PR", "METHOD", "PATH", "STA", "SIZE",
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

    // ── Spawn N workers, each with its own SO_REUSEPORT socket ────────
    let mut handles = Vec::with_capacity(num_workers * 2);

    // TCP workers (HTTP/1.1 + HTTP/2)
    for i in 0..num_workers {
        let listener = create_reuseport_listener(port)?;
        let log_mode = log_mode.clone();
        let tls_acceptor = tls_acceptor.clone();
        let mut shutdown_rx = shutdown_rx.clone();

        let handle = tokio::spawn(async move {
            loop {
                let (mut stream, addr) = tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok(conn) => conn,
                            Err(e) => {
                                eprintln!("accept error on worker {i}: {e}");
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                };

                let log_mode = log_mode.clone();
                let tls_acceptor = tls_acceptor.clone();
                tokio::task::spawn(async move {
                    // Read first byte to detect TLS vs plain HTTP.
                    // A TLS ClientHello always begins with 0x16 (ContentType::Handshake).
                    let mut first_byte = [0u8; 1];
                    let is_tls = match stream.read_exact(&mut first_byte).await {
                        Ok(_n) => first_byte[0] == TLS_CONTENT_TYPE_HANDSHAKE,
                        Err(e) => {
                            eprintln!("read error ({}): {e}", addr);
                            return;
                        }
                    };

                    if is_tls {
                        let prefixed = PrefixedStream {
                            prefix: Some(first_byte[0]),
                            inner: stream,
                        };
                        let tls_stream = match tls_acceptor.accept(prefixed).await {
                            Ok(tls) => tls,
                            Err(e) => {
                                eprintln!("TLS handshake error ({}): {e}", addr);
                                return;
                            }
                        };
                        let io = TokioIo::new(tls_stream);

                        let svc = service_fn(move |req: Request<Incoming>| {
                            let log_mode = log_mode.clone();
                            async move { handle_request(req, log_mode).await }
                        });

                        if let Err(err) = auto::Builder::new(TokioExecutor::new())
                            .serve_connection(io, svc)
                            .await
                        {
                            eprintln!("connection error ({}): {err}", addr);
                        }
                    } else {
                        let prefixed = PrefixedStream {
                            prefix: Some(first_byte[0]),
                            inner: stream,
                        };
                        let io = TokioIo::new(prefixed);

                        let svc = service_fn(move |req: Request<Incoming>| {
                            let log_mode = log_mode.clone();
                            async move { handle_request(req, log_mode).await }
                        });

                        if let Err(err) = auto::Builder::new(TokioExecutor::new())
                            .serve_connection(io, svc)
                            .await
                        {
                            eprintln!("connection error ({}): {err}", addr);
                        }
                    }
                });
            }
        });

        handles.push(handle);
    }

    // QUIC workers (HTTP/3)
    for i in 0..num_workers {
        let udp_socket = match create_reuseport_udp_socket(port) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create UDP socket for QUIC worker {i}: {e}");
                continue;
            }
        };
        let log_mode = log_mode.clone();
        let mut shutdown_rx = shutdown_rx.clone();
        let quic_tls_config: quinn::crypto::rustls::QuicServerConfig = {
            let mut quic_tls = (*tls_config).clone();
            quic_tls.alpn_protocols = vec![b"h3".to_vec()];
            match quic_tls.try_into() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to create QUIC crypto config on worker {i}: {e}");
                    continue;
                }
            }
        };
        let mut quic_server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_tls_config));
        let mut transport = quinn::TransportConfig::default();
        transport
            .max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(30_000))));
        transport.keep_alive_interval(Some(Duration::from_secs(10)));
        quic_server_config.transport_config(Arc::new(transport));

        let endpoint = match quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(quic_server_config),
            udp_socket,
            Arc::new(quinn::TokioRuntime),
        ) {
            Ok(ep) => ep,
            Err(e) => {
                eprintln!("Failed to create QUIC endpoint on worker {i}: {e}");
                continue;
            }
        };

        let handle = tokio::spawn(async move {
            loop {
                let incoming = tokio::select! {
                    result = endpoint.accept() => result,
                    _ = shutdown_rx.changed() => {
                        // Close the QUIC endpoint gracefully:
                        // sends CONNECTION_CLOSE frames to all active connections.
                        endpoint.close(0u32.into(), b"server shutting down");
                        break;
                    }
                };

                match incoming {
                    Some(incoming) => {
                        let log_mode = log_mode.clone();
                        tokio::task::spawn(async move {
                            match incoming.await {
                                Ok(conn) => {
                                    let h3_conn = H3QuinnConnection::new(conn);
                                    if let Err(e) = handle_h3_connection(h3_conn, log_mode).await {
                                        if !crate::error::is_client_cancel(&*e) {
                                            eprintln!("h3 connection error: {e}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("QUIC incoming error: {e}");
                                }
                            }
                        });
                    }
                    None => {
                        eprintln!("QUIC endpoint closed on worker {i}");
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    // ── Wait for shutdown signal ────────────────────────────────
    tokio::signal::ctrl_c().await.ok();
    eprintln!(
        "\nReceived shutdown signal — draining in-flight requests (timeout: {}s)...",
        shutdown_timeout.as_secs()
    );

    // Signal all workers to stop accepting new connections.
    let _ = shutdown_tx.send(true);
    // Drop the sender so any workers still in `changed()` see the channel as closed.
    drop(shutdown_tx);

    // Wait for workers to exit their accept loops and finish draining.
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
