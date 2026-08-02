use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{HeaderValue, CACHE_CONTROL, ETAG};
use hyper::Request;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::config::H3_HANDLERS_PER_CONNECTION;
use crate::error::is_client_cancel;
use crate::logging::{LogMode, TimedBody, TimingInfo};
use crate::{route, Asset, BUILD_VERSION};

// ── Protocol strings as static slices — no per-request allocation ─────

const PROTO_H1: &str = "h1";
const PROTO_H2: &str = "h2";
const PROTO_H3: &str = "h3";

/// Returns true if the request's `If-None-Match` header matches the build version,
/// meaning the client already has the latest version of all static resources.
#[inline]
fn is_not_modified<B>(req: &Request<B>) -> bool {
    req.headers()
        .get("if-none-match")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |etag| etag == BUILD_VERSION)
}

/// Build a 304 Not Modified response for h1/h2 path — body is empty `Full<Bytes>`.
/// Unlike the old code that constructed a `Response<Full<Bytes>>` then deconstructed
/// it, we build the response directly, avoiding one unnecessary allocation round-trip.
#[inline]
fn not_modified_response() -> hyper::Response<Full<Bytes>> {
    let mut resp = hyper::Response::new(Full::new(Bytes::new()));
    *resp.status_mut() = hyper::StatusCode::NOT_MODIFIED;
    resp.headers_mut().insert(
        ETAG,
        HeaderValue::from_static(BUILD_VERSION),
    );
    resp.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    resp
}

/// Build a 304 Not Modified response with an empty `()` body (for h3).
#[inline]
fn not_modified_response_h3() -> hyper::Response<()> {
    let mut resp = hyper::Response::new(());
    *resp.status_mut() = hyper::StatusCode::NOT_MODIFIED;
    resp.headers_mut().insert(
        ETAG,
        HeaderValue::from_static(BUILD_VERSION),
    );
    resp.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    resp
}

/// Build a full response for an asset's body (h1/h2 path).
///
/// Headers are stored as `&[(&str, &str)]` and converted to `HeaderName`/`HeaderValue`
/// at request time via `from_static` (a const fn — validation already happened at
/// compile time, so the call is just a pointer wrap). This avoids the per-request
/// `HeaderMap::clone()` hash-table allocation entirely.
#[inline]
pub fn response_for_asset(asset: &Asset) -> hyper::Response<Full<Bytes>> {
    let status =
        hyper::StatusCode::from_u16(asset.status_code).expect("invalid status code at compile time");
    let mut resp = hyper::Response::new(Full::new(Bytes::from_static(asset.body)));
    *resp.status_mut() = status;
    let headers = resp.headers_mut();
    headers.reserve(asset.headers.len());
    for &(name, value) in asset.headers {
        headers.insert(
            hyper::header::HeaderName::from_static(name),
            hyper::header::HeaderValue::from_static(value),
        );
    }
    resp
}

#[inline]
fn protocol_str(version: hyper::Version) -> &'static str {
    if version == hyper::Version::HTTP_2 {
        PROTO_H2
    } else {
        PROTO_H1
    }
}

/// Shared request handler used by both TLS and plain-HTTP connections.
/// Times from entry until the response body is consumed by hyper
/// (i.e., after the socket write), matching the h3 end-to-end scope.
pub(crate) async fn handle_request(
    req: Request<Incoming>,
    log_mode: LogMode,
) -> Result<hyper::Response<TimedBody>, Infallible> {
    let start = Instant::now();
    let path = req.uri().path();
    let method = req.method();
    let protocol = protocol_str(req.version());

    // Allocate method/path strings only when detailed logging is active;
    // summary and disabled modes only need the atomic counter.
    let (method_owned, path_owned) = match &log_mode {
        LogMode::Detailed { .. } => (method.to_string(), path.to_owned()),
        _ => (String::new(), String::new()),
    };

    // ── Generic ETag check: 304 early-return without routing ────────
    if is_not_modified(&req) {
        let resp = not_modified_response();
        let (parts, body) = resp.into_parts();
        let timed = TimedBody {
            inner: body,
            log: Some(TimingInfo {
                start,
                method: method_owned,
                path: path_owned,
                status: 304,
                size: 0,
                savings: 0,
                protocol,
                log_mode,
            }),
        };
        return Ok(hyper::Response::from_parts(parts, timed));
    }

    let asset = route(path);
    let status = asset.status_code;
    let size = asset.content_length as u64;
    let savings = asset.savings_pct as u64;
    let resp = response_for_asset(asset);
    let (parts, body) = resp.into_parts();

    let timed = TimedBody {
        inner: body,
        log: Some(TimingInfo {
            start,
            method: method_owned,
            path: path_owned,
            status,
            size,
            savings,
            protocol,
            log_mode,
        }),
    };

    Ok(hyper::Response::from_parts(parts, timed))
}

// ── h3 type aliases (module-level so helpers can reference them) ──────

type H3Stream<C> =
    h3::server::RequestStream<<C as h3::quic::OpenStreams<Bytes>>::BidiStream, Bytes>;
type H3Resolver<C> = h3::server::RequestResolver<C, Bytes>;

/// Process a single h3 request in the context of a handler-pool task.
/// Called from a pre-spawned handler — no additional `tokio::spawn` required.
async fn h3_handle_one_request<C>(
    resolver: H3Resolver<C>,
    log_mode: &LogMode,
    finished_tx: &mpsc::UnboundedSender<H3Stream<C>>,
) where
    C: h3::quic::Connection<Bytes> + 'static,
    <C as h3::quic::OpenStreams<Bytes>>::BidiStream: Send + 'static,
{
    let (req, mut stream) = match resolver.resolve_request().await {
        Ok(r) => r,
        Err(e) => {
            if !is_client_cancel(&e) {
                elog!("h3 resolve_request error: {e}");
            }
            return;
        }
    };

    // ── Full end-to-end timing (CPU + I/O, matching h1/h2) ──
    let start = Instant::now();
    let path = req.uri().path();
    let method = req.method().as_str();

    // Generic ETag check: return 304 for any resource if
    // the client already has the current build version cached.
    if is_not_modified(&req) {
        let resp = not_modified_response_h3();
        if let Err(e) = stream.send_response(resp).await {
            if !is_client_cancel(&e) {
                elog!("h3 send_response error: {e}");
            }
            return;
        }
        if let Err(e) = stream.finish().await {
            if !is_client_cancel(&e) {
                elog!("h3 finish error: {e}");
            }
            return;
        }
        let _ = finished_tx.send(stream);
        log_h3_outcome(log_mode, method, path, 304, 0, 0, start);
        return;
    }

    let asset = route(&path);
    let status_code = asset.status_code;
    let content_length = asset.content_length;
    let savings = asset.savings_pct as u64;

    let resp = h3_response_for_asset(asset);

    if let Err(e) = stream.send_response(resp).await {
        if !is_client_cancel(&e) {
            elog!("h3 send_response error: {e}");
        }
        return;
    }
    if !asset.body.is_empty() {
        if let Err(e) = stream
            .send_data(bytes::Bytes::from_static(asset.body))
            .await
        {
            if !is_client_cancel(&e) {
                elog!("h3 send_data error: {e}");
            }
            return;
        }
    }
    if let Err(e) = stream.finish().await {
        if !is_client_cancel(&e) {
            elog!("h3 finish error: {e}");
        }
        return;
    }

    // Defer drop: send the stream to the reaper channel instead
    // of letting it drop here. The main loop keeps it alive until
    // the next request arrives (or the connection closes), giving
    // Quinn's I/O driver time to transmit the FIN packet.
    let _ = finished_tx.send(stream);

    log_h3_outcome(
        log_mode,
        method,
        path,
        status_code,
        content_length as u64,
        savings,
        start,
    );
}

/// Handle a single HTTP/3 (QUIC) connection.
///
/// Instead of spawning a new `tokio::task` for every request (which adds
/// scheduler and allocator churn at high throughput), we pre-spawn a fixed
/// pool of handler tasks per connection. Requests are distributed round-robin
/// to these handlers via unbounded mpsc channels, eliminating per-request
/// task creation while preserving concurrent request processing.
///
/// Finished `RequestStream`s are sent back to this accept loop via a
/// separate channel so they can be kept alive until Quinn's I/O driver
/// has transmitted the FIN packet — preventing Safari from showing empty
/// pages over HTTP/3.
pub(crate) async fn handle_h3_connection<C>(
    conn: C,
    log_mode: LogMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    C: h3::quic::Connection<Bytes> + 'static,
    <C as h3::quic::OpenStreams<Bytes>>::BidiStream: Send + 'static,
{
    let mut h3_conn = h3::server::Connection::new(conn).await?;

    // Channel for sending finished RequestStreams back to the main loop.
    let (finished_tx, mut finished_rx) = mpsc::unbounded_channel::<H3Stream<C>>();
    // Finished streams whose FIN may not have been flushed yet.
    let mut pending_streams: Vec<H3Stream<C>> = Vec::new();

    // ── Pre-spawn handler pool (one task per slot, reused across requests) ──
    let num_handlers = H3_HANDLERS_PER_CONNECTION;
    let mut senders: Vec<mpsc::UnboundedSender<H3Resolver<C>>> =
        Vec::with_capacity(num_handlers);

    for _ in 0..num_handlers {
        let (tx, mut rx) = mpsc::unbounded_channel();
        senders.push(tx);
        let finished_tx = finished_tx.clone();
        let log_mode = log_mode.clone();
        tokio::spawn(async move {
            while let Some(resolver) = rx.recv().await {
                h3_handle_one_request::<C>(resolver, &log_mode, &finished_tx).await;
            }
        });
    }

    let mut next_handler: usize = 0;

    loop {
        // Drain finished streams — keep them alive so their RequestEnd
        // notification is not sent and the accept loop stays alive.
        while let Ok(stream) = finished_rx.try_recv() {
            pending_streams.push(stream);
        }

        match h3_conn.accept().await? {
            Some(resolver) => {
                // A new request arrived — Quinn's I/O driver has had at least
                // one full scheduling window to flush any pending FINs.
                pending_streams.clear();

                let idx = next_handler % num_handlers;
                next_handler = next_handler.wrapping_add(1);

                if senders[idx].send(resolver).is_err() {
                    elog!("h3 handler channel closed — stopping accept loop");
                    break;
                }
            }
            None => {
                // No more streams and GOAWAY received.
                break;
            }
        }
    }

    // Drop senders so handler tasks see closed channels and exit.
    drop(senders);

    Ok(())
}

// ── Extracted h3 helpers (de-duplicate the logging and response-construction logic) ──

/// Build an h3 `Response<()>` from the asset's static header slice,
/// adding the `Content-Length` header required by h3.
#[inline]
fn h3_response_for_asset(asset: &Asset) -> hyper::Response<()> {
    let status =
        hyper::StatusCode::from_u16(asset.status_code).expect("invalid status code at compile time");
    let mut resp = hyper::Response::new(());
    *resp.status_mut() = status;
    let headers = resp.headers_mut();
    // +1 for the Content-Length header we add below.
    headers.reserve(asset.headers.len() + 1);
    for &(name, value) in asset.headers {
        headers.insert(
            hyper::header::HeaderName::from_static(name),
            hyper::header::HeaderValue::from_static(value),
        );
    }
    headers.insert(
        hyper::header::CONTENT_LENGTH,
        hyper::header::HeaderValue::from_static(asset.content_length_str),
    );
    resp
}

/// Log the outcome of an h3 request — extracted from the two call sites
/// (304 and full response) to eliminate duplicated logging logic.
fn log_h3_outcome(
    log_mode: &LogMode,
    method: &str,
    path: &str,
    status: u16,
    size: u64,
    savings: u64,
    start: Instant,
) {
    match log_mode {
        LogMode::Disabled => { /* logging compiled out */ }
        LogMode::Summary(counter) => {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        LogMode::Detailed { tx, .. } => {
            let elapsed = start.elapsed().as_micros() as u64;
            let _ = tx.send((
                method.to_string(),
                path.to_owned(),
                status,
                size,
                savings,
                elapsed,
                PROTO_H3,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── not_modified_response ────────────────────────────────────

    #[test]
    fn not_modified_response_has_304_status() {
        let resp = not_modified_response();
        assert_eq!(resp.status(), hyper::StatusCode::NOT_MODIFIED);
    }

    #[test]
    fn not_modified_response_has_etag() {
        let resp = not_modified_response();
        assert_eq!(
            resp.headers().get("etag").unwrap(),
            BUILD_VERSION
        );
    }

    // ── not_modified_response_h3 ─────────────────────────────────

    #[test]
    fn not_modified_response_h3_has_304_status() {
        let resp = not_modified_response_h3();
        assert_eq!(resp.status(), hyper::StatusCode::NOT_MODIFIED);
    }

    #[test]
    fn not_modified_response_h3_has_etag() {
        let resp = not_modified_response_h3();
        assert_eq!(
            resp.headers().get("etag").unwrap(),
            BUILD_VERSION
        );
    }

    // ── is_not_modified ──────────────────────────────────────────

    #[test]
    fn is_not_modified_matching_etag_returns_true() {
        let req = Request::builder()
            .header("if-none-match", BUILD_VERSION)
            .body(())
            .unwrap();
        assert!(is_not_modified(&req));
    }

    #[test]
    fn is_not_modified_wrong_etag_returns_false() {
        let req = Request::builder()
            .header("if-none-match", "wrong-etag")
            .body(())
            .unwrap();
        assert!(!is_not_modified(&req));
    }

    #[test]
    fn is_not_modified_missing_header_returns_false() {
        let req = Request::builder().body(()).unwrap();
        assert!(!is_not_modified(&req));
    }

    // ── protocol_str ─────────────────────────────────────────────

    #[test]
    fn protocol_str_http2_returns_h2() {
        assert_eq!(protocol_str(hyper::Version::HTTP_2), "h2");
    }

    #[test]
    fn protocol_str_http11_returns_h1() {
        assert_eq!(protocol_str(hyper::Version::HTTP_11), "h1");
    }

    #[test]
    fn protocol_str_http10_returns_h1() {
        assert_eq!(protocol_str(hyper::Version::HTTP_10), "h1");
    }
}
