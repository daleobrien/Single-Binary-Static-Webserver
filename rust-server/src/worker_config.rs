use std::sync::Arc;

use crate::logging::LogMode;

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
    fn tls_handshake_byte_is_0x16() {
        assert_eq!(TLS_CONTENT_TYPE_HANDSHAKE, 0x16);
    }
}
