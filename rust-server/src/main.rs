use bytes::Bytes;
use http_body_util::Full;
use hyper::body::{Body, Frame, Incoming};
use hyper::service::service_fn;
use hyper::{HeaderMap, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use h3::server::Connection as H3Connection;
use h3_quinn::Connection as H3QuinnConnection;
use socket2::{Domain, Protocol, Socket, Type};
use std::convert::Infallible;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

const PORT: u16 = 3000;
const TLS_CONTENT_TYPE_HANDSHAKE: u8 = 0x16;

/// Logging strategy: either per-request details or a cheap atomic counter.
enum LogMode {
    Summary(Arc<AtomicU64>),
    Detailed {
        tx: mpsc::UnboundedSender<(String, String, u16, u64, u64, String)>,
        path_w: usize,
        size_w: usize,
    },
}

impl Clone for LogMode {
    fn clone(&self) -> Self {
        match self {
            Self::Summary(c) => Self::Summary(Arc::clone(c)),
            Self::Detailed { tx, path_w, size_w } => Self::Detailed {
                tx: tx.clone(),
                path_w: *path_w,
                size_w: *size_w,
            },
        }
    }
}

// ── Full-body timing for h1/h2 ────────────────────────────────────

/// Metadata needed to log a request after its response body has been
/// fully consumed by hyper (i.e., written to the socket).
struct TimingInfo {
    start: Instant,
    method: String,
    path: String,
    status: u16,
    size: u64,
    protocol: String,
    log_mode: LogMode,
}

/// Wraps a [`Full<Bytes>`] body so the elapsed time is logged when the body
/// is fully consumed by hyper. This captures the socket-write time that
/// hyper performs *after* the service handler returns, giving a true
/// end-to-end measurement comparable with the HTTP/3 path.
struct TimedBody {
    inner: Full<Bytes>,
    log: Option<TimingInfo>,
}

impl Body for TimedBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.inner).poll_frame(cx) {
            Poll::Ready(None) => {
                // Body fully consumed by hyper — data has been written to the IO stream.
                // This path fires for HTTP/1 where hyper polls until exhaustion.
                flush_log(&mut self.log);
                Poll::Ready(None)
            }
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for TimedBody {
    fn drop(&mut self) {
        // Safety net: HTTP/2 uses is_end_stream() to detect body completion
        // and may never call poll_frame a second time. When the Response is
        // dropped by hyper the socket write has already happened, matching
        // the h3 end-to-end timing scope.
        flush_log(&mut self.log);
    }
}

fn flush_log(log: &mut Option<TimingInfo>) {
    if let Some(info) = log.take() {
        let elapsed = info.start.elapsed().as_micros() as u64;
        match &info.log_mode {
            LogMode::Summary(counter) => {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            LogMode::Detailed { tx, .. } => {
                let _ = tx.send((
                    info.method, info.path, info.status, info.size,
                    elapsed, info.protocol,
                ));
            }
        }
    }
}

/// Returns `true` when the error was caused by the client cancelling
/// (e.g. browser navigated away) — not a real server error worth logging.
fn is_client_cancel(e: &dyn std::error::Error) -> bool {
    let msg = e.to_string();
    msg.contains("H3_REQUEST_CANCELLED")
        || msg.contains("h3_request_cancelled")
        || msg.contains("request cancelled")
        || msg.contains("aborted by peer")
}

// ── Compile-time generated assets ──────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[inline]
fn build_response(asset: &Asset) -> Response<Full<Bytes>> {
    let status =
        hyper::StatusCode::from_u16(asset.status_code).unwrap_or(hyper::StatusCode::OK);
    let mut resp = Response::new(Full::new(Bytes::from_static(asset.body)));
    *resp.status_mut() = status;
    *resp.headers_mut() = HEADER_MAPS[asset.header_index].clone();
    resp
}

fn create_reuseport_listener(
    port: u16,
) -> Result<TcpListener, Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    let std_listener: std::net::TcpListener = socket.into();
    Ok(TcpListener::from_std(std_listener)?)
}

fn create_reuseport_udp_socket(
    port: u16,
) -> Result<UdpSocket, Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_port(true)?;
    socket.bind(&addr.into())?;
    let std_socket: UdpSocket = socket.into();
    Ok(std_socket)
}

/// Wraps an async stream so that a previously-read single byte is yielded first,
/// then the remaining stream data follows. Used for TLS detection: we read one
/// byte to decide TLS vs plain HTTP, then re-inject it.
struct PrefixedStream<S> {
    prefix: Option<u8>,
    inner: S,
}

impl<S: AsyncRead + Unpin> AsyncRead for PrefixedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some(byte) = self.prefix.take() {
            if buf.remaining() > 0 {
                buf.put_slice(&[byte]);
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PrefixedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let summary_mode = std::env::args().any(|arg| arg == "--summary");

    eprintln!("Server running at http://localhost:{PORT}/  and  https://localhost:{PORT}/");
    eprintln!("All static files pre-compressed and embedded at compile time.");
    eprintln!(
        "{} assets baked into the binary (routing + header builders + bodies).",
        ALL_ASSETS.len()
    );
    eprintln!("Starting {num_workers} workers with SO_REUSEPORT + auto (plain HTTP, TLS h1.1/h2/h3)");
    if summary_mode {
        eprintln!("Log mode: summary (req/s every 5s)");
    }

    LazyLock::force(&HEADER_MAPS);

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install ring crypto provider");

    let tls_config = build_tls_config();
    let tls_acceptor = TlsAcceptor::from(Arc::clone(&tls_config));

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
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, String, u16, u64, u64, String)>();

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
        let listener = create_reuseport_listener(PORT)?;
        let log_mode = log_mode.clone();
        let tls_acceptor = tls_acceptor.clone();

        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, addr)) => {
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
                                    async move {
                                        handle_request(req, log_mode).await
                                    }
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
                                    async move {
                                        handle_request(req, log_mode).await
                                    }
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
                    Err(e) => {
                        eprintln!("accept error on worker {i}: {e}");
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    // QUIC workers (HTTP/3)
    for i in 0..num_workers {
        let udp_socket = match create_reuseport_udp_socket(PORT) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create UDP socket for QUIC worker {i}: {e}");
                continue;
            }
        };
        let log_mode = log_mode.clone();
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
        transport.max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(30_000))));
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
                match endpoint.accept().await {
                    Some(incoming) => {
                        let log_mode = log_mode.clone();
                        tokio::task::spawn(async move {
                            match incoming.await {
                                Ok(conn) => {
                                    let h3_conn = H3QuinnConnection::new(conn);
                                    if let Err(e) = handle_h3_connection(h3_conn, log_mode).await {
                                        if !is_client_cancel(&*e) {
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

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Shared request handler used by both TLS and plain-HTTP connections.
/// Times from entry until the response body is consumed by hyper
/// (i.e., after the socket write), matching the h3 end-to-end scope.
async fn handle_request(
    req: Request<Incoming>,
    log_mode: LogMode,
) -> Result<Response<TimedBody>, Infallible> {
    let start = Instant::now();
    let path = req.uri().path().to_owned();
    let method = req.method().to_string();
    let protocol = match req.version() {
        v if v == hyper::Version::HTTP_2 => "h2",
        _ => "h1",
    };

    // Conditional version check: return 304 if ETag matches build version
    if path == "/v" {
        if let Some(etag) = req.headers().get("if-none-match") {
            if let Ok(etag_str) = etag.to_str() {
                if etag_str == BUILD_VERSION {
                    let mut resp = Response::new(Full::new(Bytes::new()));
                    *resp.status_mut() = hyper::StatusCode::NOT_MODIFIED;
                    resp.headers_mut().insert(
                        HeaderName::from_static("etag"),
                        HeaderValue::from_static(BUILD_VERSION),
                    );
                    resp.headers_mut().insert(
                        HeaderName::from_static("cache-control"),
                        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
                    );
                    let (parts, body) = resp.into_parts();
                    let timed = TimedBody {
                        inner: body,
                        log: Some(TimingInfo {
                            start,
                            method,
                            path,
                            status: 304,
                            size: 0,
                            protocol: protocol.to_string(),
                            log_mode,
                        }),
                    };
                    return Ok(Response::from_parts(parts, timed));
                }
            }
        }
    }

    let asset = route(&path);
    let status = asset.status_code;
    let size = asset.content_length as u64;
    let resp = build_response(asset);

    let (parts, body) = resp.into_parts();
    let timed = TimedBody {
        inner: body,
        log: Some(TimingInfo {
            start,
            method,
            path,
            status,
            size,
            protocol: protocol.to_string(),
            log_mode,
        }),
    };

    Ok(Response::from_parts(parts, timed))
}

/// Handle a single HTTP/3 (QUIC) connection.
/// Spawns a task per request but defers dropping each `RequestStream`. This
/// prevents the `RequestEnd` notification from reaching the accept loop before
/// Quinn's I/O driver has transmitted the FIN packet — the root cause of
/// Safari showing empty pages over HTTP/3.
async fn handle_h3_connection<C>(
    conn: C,
    log_mode: LogMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    C: h3::quic::Connection<Bytes> + 'static,
    <C as h3::quic::OpenStreams<Bytes>>::BidiStream: Send + 'static,
{
    let mut h3_conn = H3Connection::new(conn).await?;

    // Channel for sending finished RequestStreams back to the main loop
    // so they can be kept alive until the Quinn I/O driver catches up.
    type H3Stream<C> = h3::server::RequestStream<
        <C as h3::quic::OpenStreams<Bytes>>::BidiStream,
        Bytes,
    >;
    let (finished_tx, mut finished_rx) = mpsc::unbounded_channel::<H3Stream<C>>();
    // Finished streams whose FIN may not have been flushed yet.
    let mut pending_streams: Vec<H3Stream<C>> = Vec::new();

    loop {
        // Drain any streams that spawned tasks have finished and sent back.
        // We keep them alive here (in pending_streams) so their RequestEnd
        // notification is NOT sent and the accept loop stays alive.
        while let Ok(stream) = finished_rx.try_recv() {
            pending_streams.push(stream);
        }

        match h3_conn.accept().await? {
            Some(resolver) => {
                // A new request arrived — Quinn's I/O driver has had at least
                // one full scheduling window to flush any pending FINs from
                // previous requests. It's now safe to drop old streams.
                pending_streams.clear();

                let log_mode = log_mode.clone();
                let finished_tx = finished_tx.clone();
                tokio::spawn(async move {
                    let (req, mut stream) = match resolver.resolve_request().await {
                        Ok(r) => r,
                        Err(e) => {
                            if !is_client_cancel(&e) {
                                eprintln!("h3 resolve_request error: {e}");
                            }
                            return;
                        }
                    };

                    // ── Full end-to-end timing (CPU + I/O, matching h1/h2) ──
                    let start = Instant::now();
                    let path = req.uri().path().to_owned();
                    let method = req.method().to_string();

                    // Conditional version check: return 304 if ETag matches
                    if path == "/v" {
                        if let Some(etag) = req.headers().get("if-none-match") {
                            if let Ok(etag_str) = etag.to_str() {
                                if etag_str == BUILD_VERSION {
                                    let mut resp = hyper::Response::new(());
                                    *resp.status_mut() = hyper::StatusCode::NOT_MODIFIED;
                                    resp.headers_mut().insert(
                                        hyper::header::HeaderName::from_static("etag"),
                                        hyper::header::HeaderValue::from_static(BUILD_VERSION),
                                    );
                                    resp.headers_mut().insert(
                                        hyper::header::HeaderName::from_static("cache-control"),
                                        hyper::header::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
                                    );
                                    if let Err(e) = stream.send_response(resp).await {
                                        if !is_client_cancel(&e) {
                                            eprintln!("h3 send_response error: {e}");
                                        }
                                        return;
                                    }
                                    if let Err(e) = stream.finish().await {
                                        if !is_client_cancel(&e) {
                                            eprintln!("h3 finish error: {e}");
                                        }
                                        return;
                                    }
                                    let _ = finished_tx.send(stream);
                                    match &log_mode {
                                        LogMode::Summary(counter) => {
                                            counter.fetch_add(1, Ordering::Relaxed);
                                        }
                                        LogMode::Detailed { tx, .. } => {
                                            let elapsed = start.elapsed().as_micros() as u64;
                                            let _ = tx.send((
                                                method,
                                                path,
                                                304_u16,
                                                0_u64,
                                                elapsed,
                                                "h3".to_string(),
                                            ));
                                        }
                                    }
                                    return;
                                }
                            }
                        }
                    }

                    let asset = route(&path);
                    let status_code = asset.status_code;
                    let content_length = asset.content_length;

                    let status = hyper::StatusCode::from_u16(asset.status_code)
                        .unwrap_or(hyper::StatusCode::OK);
                    let mut resp = hyper::Response::new(());
                    *resp.status_mut() = status;
                    *resp.headers_mut() = HEADER_MAPS[asset.header_index].clone();
                    resp.headers_mut().insert(
                        hyper::header::CONTENT_LENGTH,
                        hyper::header::HeaderValue::from_str(&content_length.to_string())
                            .unwrap_or(hyper::header::HeaderValue::from_static("0")),
                    );

                    if let Err(e) = stream.send_response(resp).await {
                        if !is_client_cancel(&e) {
                            eprintln!("h3 send_response error: {e}");
                        }
                        return;
                    }
                    if !asset.body.is_empty() {
                        if let Err(e) = stream.send_data(bytes::Bytes::from_static(asset.body)).await {
                            if !is_client_cancel(&e) {
                                eprintln!("h3 send_data error: {e}");
                            }
                            return;
                        }
                    }
                    if let Err(e) = stream.finish().await {
                        if !is_client_cancel(&e) {
                            eprintln!("h3 finish error: {e}");
                        }
                        return;
                    }

                    // Defer drop: send the stream to the reaper channel instead
                    // of letting it drop here. The main loop keeps it alive until
                    // the next request arrives (or the connection closes), giving
                    // Quinn's I/O driver time to transmit the FIN packet.
                    let _ = finished_tx.send(stream);

                    // Log full end-to-end: CPU + QUIC I/O, comparable to h1/h2.
                    match &log_mode {
                        LogMode::Summary(counter) => {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                        LogMode::Detailed { tx, .. } => {
                            let elapsed = start.elapsed().as_micros() as u64;
                            let _ = tx.send((
                                method,
                                path,
                                status_code,
                                content_length as u64,
                                elapsed,
                                "h3".to_string(),
                            ));
                        }
                    }
                });
            }
            None => {
                // No more streams and GOAWAY received. Drop any pending
                // streams and let the connection close naturally.
                break;
            }
        }
    }

    Ok(())
}
