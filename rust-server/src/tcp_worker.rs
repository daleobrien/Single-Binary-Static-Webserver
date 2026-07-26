use std::sync::Arc;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::Request;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use crate::config::{MAX_CONNECTIONS, TLS_CONTENT_TYPE_HANDSHAKE, WorkerConfig};
use crate::handlers::handle_request;
use crate::logging::LogMode;
use crate::sockets::create_reuseport_listener;
use crate::tls_stream::PrefixedStream;

/// Handle a single accepted TCP connection: sniff the first byte to detect
/// TLS vs plain HTTP, then serve with hyper's auto-detection (h1/h2).
async fn handle_tcp_connection(
    mut stream: TcpStream,
    addr: std::net::SocketAddr,
    tls_acceptor: TlsAcceptor,
    log_mode: LogMode,
) {
    let mut first_byte = [0u8; 1];
    let is_tls = match stream.read_exact(&mut first_byte).await {
        Ok(_n) => first_byte[0] == TLS_CONTENT_TYPE_HANDSHAKE,
        Err(e) => {
            eprintln!("read error ({}): {e}", addr);
            return;
        }
    };

    // Disable Nagle's algorithm for lower latency on HTTP responses.
    if let Err(e) = stream.set_nodelay(true) {
        eprintln!("set_nodelay error ({}): {e}", addr);
    }

    let prefixed = PrefixedStream {
        prefix: Some(first_byte[0]),
        inner: stream,
    };

    // Build the HTTP service once, then serve via TLS or plain depending
    // on the detected protocol. The service_fn is shared between branches;
    // only `serve_connection` must be duplicated because the IO types differ.
    let svc = service_fn(move |req: Request<Incoming>| {
        let log_mode = log_mode.clone();
        async move { handle_request(req, log_mode).await }
    });

    let result = if is_tls {
        let tls_stream = match tls_acceptor.accept(prefixed).await {
            Ok(tls) => tls,
            Err(e) => {
                eprintln!("TLS handshake error ({}): {e}", addr);
                return;
            }
        };
        auto::Builder::new(TokioExecutor::new())
            .serve_connection(TokioIo::new(tls_stream), svc)
            .await
    } else {
        auto::Builder::new(TokioExecutor::new())
            .serve_connection(TokioIo::new(prefixed), svc)
            .await
    };

    if let Err(err) = result {
        eprintln!("connection error ({}): {err}", addr);
    }
}

/// Spawn `num_workers` TCP listener tasks, each on its own SO_REUSEPORT
/// socket. Incoming connections are limited by a shared semaphore to
/// prevent unbounded resource usage under load. Returns handles that can
/// be awaited for graceful shutdown.
pub(crate) fn spawn_tcp_workers(
    cfg: WorkerConfig,
) -> Result<
    Vec<tokio::task::JoinHandle<()>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let WorkerConfig {
        num_workers,
        port,
        tls_config,
        log_mode,
        shutdown_rx,
    } = cfg;

    let mut handles = Vec::with_capacity(num_workers);

    let tls_acceptor = TlsAcceptor::from(Arc::clone(&tls_config));
    let conn_semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    for i in 0..num_workers {
        let listener = create_reuseport_listener(port)?;
        let log_mode = log_mode.clone();
        let tls_acceptor = tls_acceptor.clone();
        let mut shutdown_rx = shutdown_rx.clone();
        let conn_semaphore = Arc::clone(&conn_semaphore);

        let handle = tokio::spawn(async move {
            loop {
                let (stream, addr) = tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok(conn) => conn,
                            Err(e) => {
                                eprintln!("accept error on TCP worker {i}: {e}");
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                };

                // Acquire a connection permit before spawning; this provides
                // backpressure when the concurrent-connection limit is reached.
                let permit = tokio::select! {
                    permit = conn_semaphore.clone().acquire_owned() => {
                        match permit {
                            Ok(p) => p,
                            Err(_) => break,
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                };

                let log_mode = log_mode.clone();
                let tls_acceptor = tls_acceptor.clone();
                tokio::task::spawn(async move {
                    let _permit = permit;
                    handle_tcp_connection(stream, addr, tls_acceptor, log_mode).await;
                });
            }
        });

        handles.push(handle);
    }

    Ok(handles)
}
