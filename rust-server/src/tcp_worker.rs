use std::sync::Arc;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::Request;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, OwnedSemaphorePermit};
use tokio_rustls::TlsAcceptor;

use crate::config::{
    H2_CONN_WINDOW, H2_MAX_FRAME_SIZE, H2_MAX_SEND_BUF, H2_STREAM_WINDOW, MAX_CONNECTIONS,
    TCP_HANDLERS_PER_WORKER,
};
use crate::worker_config::{TLS_CONTENT_TYPE_HANDSHAKE, WorkerConfig};
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
            elog!("read error ({}): {e}", addr);
            return;
        }
    };

    // Disable Nagle's algorithm for lower latency on HTTP responses.
    if let Err(e) = stream.set_nodelay(true) {
        elog!("set_nodelay error ({}): {e}", addr);
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
        async move { handle_request(req, addr, log_mode).await }
    });

    // ── HTTP/2 tuning: larger flow-control windows eliminate
    // WINDOW_UPDATE round trips when browsers load pages with many
    // concurrent streams; larger frames reduce per-frame overhead
    // for bigger bodies; a larger send buffer prevents mid-response
    // stalls on assets > 400 KB (the hyper default).
    let mut conn_builder = auto::Builder::new(TokioExecutor::new());
    {
        let mut h2 = conn_builder.http2();
        h2.initial_connection_window_size(H2_CONN_WINDOW)
            .initial_stream_window_size(H2_STREAM_WINDOW)
            .max_frame_size(Some(H2_MAX_FRAME_SIZE))
            .max_send_buf_size(H2_MAX_SEND_BUF);
    }

    let result = if is_tls {
        let tls_stream = match tls_acceptor.accept(prefixed).await {
            Ok(tls) => tls,
            Err(e) => {
                elog!("TLS handshake error ({}): {e}", addr);
                return;
            }
        };
        conn_builder
            .serve_connection(TokioIo::new(tls_stream), svc)
            .await
    } else {
        conn_builder
            .serve_connection(TokioIo::new(prefixed), svc)
            .await
    };

    if let Err(err) = result {
        elog!("connection error ({}): {err}", addr);
    }
}

/// Spawn `num_workers` TCP listener tasks, each on its own SO_REUSEPORT
/// socket. Instead of spawning a new Tokio task per accepted connection
/// (which adds scheduler overhead at very high connection rates), each
/// worker maintains a fixed-size pool of handler tasks. Accepted
/// connections are distributed to handlers via round-robin over
/// unbounded mpsc channels. The global connection semaphore still
/// provides backpressure.
///
/// Returns handles that can be awaited for graceful shutdown.
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

    let tls_acceptor = TlsAcceptor::from(Arc::clone(&tls_config));
    let conn_semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    // Each worker gets a fixed pool of handler tasks. This eliminates the
    // per-connection `tokio::task::spawn` overhead while still processing
    // multiple connections concurrently within each worker.
    let handlers_per_worker = TCP_HANDLERS_PER_WORKER;

    // Pre-allocate handles; each worker contributes 1 accept task +
    // `handlers_per_worker` handler tasks.
    let total_tasks = num_workers * (1 + handlers_per_worker);
    let mut handles = Vec::with_capacity(total_tasks);

    for i in 0..num_workers {
        let listener = create_reuseport_listener(port)?;
        let log_mode = log_mode.clone();
        let tls_acceptor = tls_acceptor.clone();
        let mut shutdown_rx = shutdown_rx.clone();
        let conn_semaphore = Arc::clone(&conn_semaphore);

        // ── Handler task pool ──────────────────────────────────────
        // Each handler gets its own mpsc channel so the accept loop can
        // round-robin connections without contention.
        let mut senders: Vec<mpsc::UnboundedSender<(TcpStream, std::net::SocketAddr, OwnedSemaphorePermit)>> =
            Vec::with_capacity(handlers_per_worker);

        for _h in 0..handlers_per_worker {
            let (tx, mut rx) = mpsc::unbounded_channel();
            senders.push(tx);

            let tls = tls_acceptor.clone();
            let log = log_mode.clone();

            let handle = tokio::spawn(async move {
                while let Some((stream, addr, permit)) = rx.recv().await {
                    let _permit = permit;
                    handle_tcp_connection(stream, addr, tls.clone(), log.clone()).await;
                }
            });
            handles.push(handle);
        }

        // ── Accept loop ────────────────────────────────────────────
        let handle = tokio::spawn(async move {
            let mut next_handler: usize = 0;

            loop {
                let (stream, addr) = tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok(conn) => conn,
                            Err(e) => {
                                elog!("accept error on TCP worker {i}: {e}");
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                };

                // Acquire a connection permit before dispatching; this
                // provides backpressure when the concurrent-connection
                // limit is reached.
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

                let idx = next_handler % handlers_per_worker;
                next_handler = next_handler.wrapping_add(1);

                // If a handler task has died (channel closed), stop.
                if senders[idx]
                    .send((stream, addr, permit))
                    .is_err()
                {
                    elog!("TCP handler channel closed on worker {i} — stopping accept loop");
                    break;
                }
            }

            // Drop all senders so the handler tasks see the channel as
            // closed and exit their recv loops.
            drop(senders);
        });

        handles.push(handle);
    }

    Ok(handles)
}
