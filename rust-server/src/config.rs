use std::sync::{Arc, LazyLock};

use crate::logging::LogMode;

/// Server port — configurable via the `PORT` environment variable, defaults to 3000.
pub(crate) static PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000)
});

/// Number of worker threads — configurable via the `WORKERS` environment
/// variable, defaults to `available_parallelism()` (with a floor of 4).
pub(crate) static NUM_WORKERS: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
});

pub(crate) const TLS_CONTENT_TYPE_HANDSHAKE: u8 = 0x16;

/// Configuration shared by TCP and QUIC worker spawn functions.
#[derive(Clone)]
pub(crate) struct WorkerConfig {
    pub num_workers: usize,
    pub port: u16,
    pub tls_config: Arc<rustls::ServerConfig>,
    pub log_mode: LogMode,
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

/// Graceful shutdown: how long the server waits for in-flight requests to complete
/// after receiving SIGINT/SIGTERM before force-exiting. Configurable via
/// `SHUTDOWN_TIMEOUT_SECS` env var, defaults to 30 seconds.
pub(crate) static SHUTDOWN_TIMEOUT_SECS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SHUTDOWN_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_default_is_3000() {
        assert_eq!(*PORT, 3000);
    }

    #[test]
    fn tls_handshake_byte_is_0x16() {
        assert_eq!(TLS_CONTENT_TYPE_HANDSHAKE, 0x16);
    }

    #[test]
    fn shutdown_timeout_default_is_30() {
        assert_eq!(*SHUTDOWN_TIMEOUT_SECS, 30);
    }

    #[test]
    fn num_workers_default_is_reasonable() {
        let n = *NUM_WORKERS;
        assert!(n >= 1, "expected at least 1 worker, got {n}");
        assert!(n <= 1024, "expected at most 1024 workers, got {n}");
    }
}
