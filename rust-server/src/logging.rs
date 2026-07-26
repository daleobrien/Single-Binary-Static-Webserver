use bytes::Bytes;
use http_body_util::Full;
use hyper::body::{Body, Frame};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;
use tokio::sync::mpsc;

/// Logging strategy: either per-request details or a cheap atomic counter.
pub(crate) enum LogMode {
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
pub(crate) struct TimingInfo {
    pub(crate) start: Instant,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) status: u16,
    pub(crate) size: u64,
    pub(crate) protocol: String,
    pub(crate) log_mode: LogMode,
}

/// Wraps a [`Full<Bytes>`] body so the elapsed time is logged when the body
/// is fully consumed by hyper. This captures the socket-write time that
/// hyper performs *after* the service handler returns, giving a true
/// end-to-end measurement comparable with the HTTP/3 path.
pub(crate) struct TimedBody {
    pub(crate) inner: Full<Bytes>,
    pub(crate) log: Option<TimingInfo>,
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

pub(crate) fn flush_log(log: &mut Option<TimingInfo>) {
    if let Some(info) = log.take() {
        let elapsed = info.start.elapsed().as_micros() as u64;
        match &info.log_mode {
            LogMode::Summary(counter) => {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            LogMode::Detailed { tx, .. } => {
                let _ = tx.send((
                    info.method,
                    info.path,
                    info.status,
                    info.size,
                    elapsed,
                    info.protocol,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // ── LogMode Clone ────────────────────────────────────────────

    #[test]
    fn log_mode_clone_summary_shares_atomic_counter() {
        let counter = Arc::new(AtomicU64::new(0));
        let a = LogMode::Summary(Arc::clone(&counter));
        let b = a.clone();
        if let LogMode::Summary(c) = &b {
            c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn log_mode_clone_detailed_shares_sender_and_preserves_widths() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let a = LogMode::Detailed {
            tx,
            path_w: 42,
            size_w: 7,
        };
        let b = a.clone();
        if let LogMode::Detailed { tx, path_w, size_w } = &b {
            assert_eq!(*path_w, 42);
            assert_eq!(*size_w, 7);
            tx.send(("GET".into(), "/t".into(), 200, 100_u64, 50_u64, "h1".into()))
                .unwrap();
        }
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.0, "GET");
        assert_eq!(msg.1, "/t");
        assert_eq!(msg.2, 200);
    }

    // ── flush_log: dispatches to summary or detailed logging ─────

    #[test]
    fn flush_log_summary_increments_counter_and_consumes_option() {
        let counter = Arc::new(AtomicU64::new(0));
        let info = TimingInfo {
            start: std::time::Instant::now(),
            method: "GET".into(),
            path: "/".into(),
            status: 200,
            size: 1024,
            protocol: "h1".into(),
            log_mode: LogMode::Summary(Arc::clone(&counter)),
        };
        let mut log = Some(info);
        flush_log(&mut log);
        assert!(log.is_none());
        assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn flush_log_detailed_sends_on_channel() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let info = TimingInfo {
            start: std::time::Instant::now(),
            method: "POST".into(),
            path: "/api".into(),
            status: 201,
            size: 512,
            protocol: "h2".into(),
            log_mode: LogMode::Detailed {
                tx,
                path_w: 10,
                size_w: 5,
            },
        };
        let mut log = Some(info);
        flush_log(&mut log);
        assert!(log.is_none());

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.0, "POST");
        assert_eq!(msg.1, "/api");
        assert_eq!(msg.2, 201);
        assert_eq!(msg.5, "h2");
        // elapsed microseconds (0 is valid for instantaneous operations)
        assert_eq!(msg.5, "h2");
    }

    #[test]
    fn flush_log_none_is_noop() {
        let mut log: Option<TimingInfo> = None;
        flush_log(&mut log);
        assert!(log.is_none());
    }
}
