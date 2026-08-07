// ── Compile-time generated config constants ─────────────────────
include!(concat!(env!("OUT_DIR"), "/config_constants.rs"));

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
    fn shutdown_timeout_default_is_3() {
        assert_eq!(SHUTDOWN_TIMEOUT_SECS, 3);
    }

    #[test]
    fn num_workers_default_is_reasonable() {
        assert!(NUM_WORKERS >= 1, "expected at least 1 worker, got {NUM_WORKERS}");
        assert!(NUM_WORKERS <= 1024, "expected at most 1024 workers, got {NUM_WORKERS}");
    }

    #[test]
    fn max_connections_default_is_4096() {
        assert_eq!(MAX_CONNECTIONS, 4096);
    }

    #[test]
    fn tcp_handlers_per_worker_default_is_max_of_quotient_or_64() {
        let expected = (MAX_CONNECTIONS / NUM_WORKERS).max(64);
        assert_eq!(
            TCP_HANDLERS_PER_WORKER, expected,
            "TCP_HANDLERS_PER_WORKER should be max(MAX_CONNECTIONS / NUM_WORKERS, 64)"
        );
    }

    #[test]
    fn h3_handlers_per_connection_default_is_8() {
        assert_eq!(H3_HANDLERS_PER_CONNECTION, 8);
    }

    #[test]
    fn h2_conn_window_default_is_16_mib() {
        assert_eq!(H2_CONN_WINDOW, 16 * 1024 * 1024);
    }

    #[test]
    fn h2_stream_window_default_is_4_mib() {
        assert_eq!(H2_STREAM_WINDOW, 4 * 1024 * 1024);
    }

    #[test]
    fn h2_max_frame_size_default_is_65535() {
        assert_eq!(H2_MAX_FRAME_SIZE, 65_535);
    }

    #[test]
    fn h2_max_send_buf_default_is_1_mib() {
        assert_eq!(H2_MAX_SEND_BUF, 1024 * 1024);
    }

    #[test]
    fn disable_logging_default_is_true() {
        assert!(DISABLE_LOGGING);
    }
}
