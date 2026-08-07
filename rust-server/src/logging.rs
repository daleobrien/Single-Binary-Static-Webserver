use bytes::Bytes;
use http_body_util::Full;
use hyper::body::{Body, Frame};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

/// Logging strategy: either per-request details, a cheap atomic counter,
/// or completely disabled (when `DISABLE_LOGGING=true` at build time).
pub(crate) enum LogMode {
    Disabled,
    Summary(Arc<AtomicU64>),
    Detailed {
        /// Channel: (remote_addr, method, path, status, size, elapsed_us, protocol)
        tx: mpsc::UnboundedSender<(SocketAddr, String, String, u16, u64, u64, &'static str)>,
    },
}

impl Clone for LogMode {
    fn clone(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::Summary(c) => Self::Summary(Arc::clone(c)),
            Self::Detailed { tx } => Self::Detailed {
                tx: tx.clone(),
            },
        }
    }
}

// ── Full-body timing for h1/h2 ────────────────────────────────────

/// Metadata needed to log a request after its response body has been
/// fully consumed by hyper (i.e., written to the socket).
pub(crate) struct TimingInfo {
    pub(crate) start: Instant,
    pub(crate) remote_addr: SocketAddr,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) status: u16,
    pub(crate) size: u64,
    pub(crate) protocol: &'static str,
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

// ── CLF timestamp formatting ──────────────────────────────────────

/// Format the current UTC time as a Common Log Format timestamp:
/// `DD/Mon/YYYY:HH:MM:SS +0000`
///
/// Uses Howard Hinnant's `civil_from_days` algorithm to convert the Unix
/// timestamp to year/month/day without any external dependencies.
fn format_clf_time() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let time_of_day = secs % 86_400;
    let hours = time_of_day / 3_600;
    let minutes = (time_of_day % 3_600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 → days since 0000-03-01 (Hinnant's epoch)
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;

    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    static MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    format!(
        "{:02}/{}/{:04}:{:02}:{:02}:{:02} +0000",
        d,
        MONTHS[(m - 1) as usize],
        y,
        hours,
        minutes,
        seconds,
    )
}

/// Produce a Common Log Format line for a single request.
///
/// CLF format:
/// `remote_addr - - [DD/Mon/YYYY:HH:MM:SS +0000] "METHOD /path HTTP/x.y" status size`
fn format_clf_entry(
    remote_addr: SocketAddr,
    method: &str,
    path: &str,
    protocol: &str,
    status: u16,
    size: u64,
) -> String {
    format!(
        "{} - - [{}] \"{} {} {}\" {} {}",
        remote_addr.ip(),
        format_clf_time(),
        method,
        path,
        protocol,
        status,
        size,
    )
}

// ── Logging initialisation ────────────────────────────────────────

/// Initialise the logging subsystem: spawns a background task that either
/// counts requests for `--summary` mode or batches per-request details for
/// the detailed mode. Returns the `LogMode` handle to pass to workers along
/// with the background task's `JoinHandle` so it can be awaited during
/// graceful shutdown.
pub(crate) fn init_logging(
    summary_mode: bool,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> (LogMode, tokio::task::JoinHandle<()>) {
    if crate::config::DISABLE_LOGGING {
        // No-op background task — resolves immediately on drop/await.
        let handle = tokio::spawn(async {});
        return (LogMode::Disabled, handle);
    }

    if summary_mode {
        let counter = Arc::new(AtomicU64::new(0));
        let counter_bg = Arc::clone(&counter);

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let count = counter_bg.swap(0, Ordering::Relaxed);
                        elog!(
                            "{count} requests in the last 5s ({:.1} req/s)",
                            count as f64 / 5.0
                        );
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        });

        (LogMode::Summary(counter), handle)
    } else {
        let (tx, mut rx) =
            mpsc::unbounded_channel::<(SocketAddr, String, String, u16, u64, u64, &'static str)>();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.tick().await;
            // Pre-allocate batch vec with a reasonable capacity to avoid
            // repeated reallocations under load. Even if we over-allocate,
            // the vec is short-lived (dropped after each 1s interval).
            let mut batch: Vec<(SocketAddr, String, String, u16, u64, u64, &'static str)> =
                Vec::with_capacity(1024);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        batch.clear();
                        while let Ok(entry) = rx.try_recv() {
                            batch.push(entry);
                        }

                        for (remote_addr, method, path, status, size, _elapsed_us, protocol) in &batch {
                            elog!(
                                "{}",
                                format_clf_entry(*remote_addr, method, path, protocol, *status, *size)
                            );
                        }
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        });

        (LogMode::Detailed { tx }, handle)
    }
}

pub(crate) fn flush_log(log: &mut Option<TimingInfo>) {
    if let Some(info) = log.take() {
        let elapsed = info.start.elapsed().as_micros() as u64;
        match &info.log_mode {
            LogMode::Disabled => { /* logging compiled out */ }
            LogMode::Summary(counter) => {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            LogMode::Detailed { tx } => {
                let _ = tx.send((
                    info.remote_addr,
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
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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
    fn log_mode_clone_detailed_shares_sender() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let a = LogMode::Detailed { tx };
        let b = a.clone();
        if let LogMode::Detailed { tx } = &b {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 54321);
            tx.send((addr, "GET".into(), "/t".into(), 200, 100_u64, 30_u64, "HTTP/1.1"))
                .unwrap();
        }
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.1, "GET");
        assert_eq!(msg.2, "/t");
        assert_eq!(msg.3, 200);
    }

    // ── flush_log: dispatches to summary or detailed logging ─────

    #[test]
    fn flush_log_summary_increments_counter_and_consumes_option() {
        let counter = Arc::new(AtomicU64::new(0));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
        let info = TimingInfo {
            start: std::time::Instant::now(),
            remote_addr: addr,
            method: "GET".into(),
            path: "/".into(),
            status: 200,
            size: 1024,
            protocol: "HTTP/1.1",
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
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 8080);
        let info = TimingInfo {
            start: std::time::Instant::now(),
            remote_addr: addr,
            method: "POST".into(),
            path: "/api".into(),
            status: 201,
            size: 512,
            protocol: "HTTP/2.0",
            log_mode: LogMode::Detailed { tx },
        };
        let mut log = Some(info);
        flush_log(&mut log);
        assert!(log.is_none());

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.0, addr);
        assert_eq!(msg.1, "POST");
        assert_eq!(msg.2, "/api");
        assert_eq!(msg.3, 201);
        assert_eq!(msg.6, "HTTP/2.0");
    }

    #[test]
    fn flush_log_none_is_noop() {
        let mut log: Option<TimingInfo> = None;
        flush_log(&mut log);
        assert!(log.is_none());
    }

    // ── format_clf_entry ─────────────────────────────────────────

    #[test]
    fn clf_entry_has_expected_structure() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 54321);
        // We can't test the exact timestamp, but we can test the surrounding format.
        let entry = format_clf_entry(addr, "GET", "/index.html", "HTTP/1.1", 200, 1234);
        assert!(entry.starts_with("127.0.0.1 - - ["));
        assert!(entry.contains("] \"GET /index.html HTTP/1.1\" 200 1234"));
    }
}
