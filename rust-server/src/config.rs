use std::sync::Arc;

use crate::logging::LogMode;

// ── Compile-time generated config constants ─────────────────────
include!(concat!(env!("OUT_DIR"), "/config_constants.rs"));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_default_is_localhost() {
        assert_eq!(HOSTNAME, "localhost");
    }

    #[test]
    fn port_default_is_3000() {
        assert_eq!(PORT, 3000);
    }

    #[test]
    fn tls_handshake_byte_is_0x16() {
        assert_eq!(TLS_CONTENT_TYPE_HANDSHAKE, 0x16);
    }

    #[test]
    fn shutdown_timeout_default_is_3() {
        assert_eq!(SHUTDOWN_TIMEOUT_SECS, 3);
    }

    #[test]
    fn num_workers_default_is_reasonable() {
        assert!(NUM_WORKERS >= 1, "expected at least 1 worker, got {NUM_WORKERS}");
        assert!(NUM_WORKERS <= 1024, "expected at most 1024 workers, got {NUM_WORKERS}");
    }
}
