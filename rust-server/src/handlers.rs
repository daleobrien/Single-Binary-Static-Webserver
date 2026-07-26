use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::Request;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::error::is_client_cancel;
use crate::logging::{LogMode, TimedBody, TimingInfo};
use crate::{Asset, BUILD_VERSION, HEADER_MAPS, route};

#[inline]
pub(crate) fn build_response(asset: &Asset) -> hyper::Response<Full<Bytes>> {
    let status =
        hyper::StatusCode::from_u16(asset.status_code).unwrap_or(hyper::StatusCode::OK);
    let mut resp = hyper::Response::new(Full::new(Bytes::from_static(asset.body)));
    *resp.status_mut() = status;
    *resp.headers_mut() = HEADER_MAPS[asset.header_index].clone();
    resp
}

/// Shared request handler used by both TLS and plain-HTTP connections.
/// Times from entry until the response body is consumed by hyper
/// (i.e., after the socket write), matching the h3 end-to-end scope.
pub(crate) async fn handle_request(
    req: Request<Incoming>,
    log_mode: LogMode,
) -> Result<hyper::Response<TimedBody>, Infallible> {
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
                    let mut resp = hyper::Response::new(Full::new(Bytes::new()));
                    *resp.status_mut() = hyper::StatusCode::NOT_MODIFIED;
                    resp.headers_mut().insert(
                        hyper::header::HeaderName::from_static("etag"),
                        hyper::header::HeaderValue::from_static(BUILD_VERSION),
                    );
                    resp.headers_mut().insert(
                        hyper::header::HeaderName::from_static("cache-control"),
                        hyper::header::HeaderValue::from_static(
                            "no-cache, no-store, must-revalidate",
                        ),
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
                    return Ok(hyper::Response::from_parts(parts, timed));
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

    Ok(hyper::Response::from_parts(parts, timed))
}

/// Handle a single HTTP/3 (QUIC) connection.
/// Spawns a task per request but defers dropping each `RequestStream`. This
/// prevents the `RequestEnd` notification from reaching the accept loop before
/// Quinn's I/O driver has transmitted the FIN packet — the root cause of
/// Safari showing empty pages over HTTP/3.
pub(crate) async fn handle_h3_connection<C>(
    conn: C,
    log_mode: LogMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    C: h3::quic::Connection<Bytes> + 'static,
    <C as h3::quic::OpenStreams<Bytes>>::BidiStream: Send + 'static,
{
    let mut h3_conn = h3::server::Connection::new(conn).await?;

    // Channel for sending finished RequestStreams back to the main loop
    // so they can be kept alive until the Quinn I/O driver catches up.
    type H3Stream<C> = h3::server::RequestStream<
        <C as h3::quic::OpenStreams<Bytes>>::BidiStream,
        Bytes,
    >;
    let (finished_tx, mut finished_rx) = tokio::sync::mpsc::unbounded_channel::<H3Stream<C>>();
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
                                        hyper::header::HeaderValue::from_static(
                                            "no-cache, no-store, must-revalidate",
                                        ),
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
                        if let Err(e) =
                            stream.send_data(bytes::Bytes::from_static(asset.body)).await
                        {
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
