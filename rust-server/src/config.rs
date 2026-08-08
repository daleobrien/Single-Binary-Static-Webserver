// ── Compile-time generated config constants ─────────────────────
include!(concat!(env!("OUT_DIR"), "/config_constants.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // Test strategy:
    //
    // Each config constant has TWO tests:
    //   1. A "default" test  — asserts the default value, but only when
    //      the ENV var is NOT set (otherwise the constant may differ).
    //   2. An "override" test — asserts the constant matches the ENV
    //      var, but only when the ENV var IS set.
    //
    // In a clean environment, default tests assert and override tests
    // are no-ops.  With an ENV var set, the situation reverses.  Both
    // scenarios are covered — you can run `cargo test` with or without
    // custom env vars and the relevant assertions will fire.
    // ═══════════════════════════════════════════════════════════════

    // ── HOSTNAME ──────────────────────────────────────────────────

    #[test]
    fn hostname_default_is_localhost() {
        if option_env!("HOSTNAME").is_none() {
            assert_eq!(HOSTNAME, "localhost");
        }
    }

    #[test]
    fn override_hostname_matches_env() {
        if let Some(expected) = option_env!("HOSTNAME") {
            assert_eq!(HOSTNAME, expected,
                "HOSTNAME constant must match HOSTNAME env var");
        }
    }

    // ── PORT ──────────────────────────────────────────────────────

    #[test]
    fn port_default_is_3000() {
        if option_env!("PORT").is_none() {
            assert_eq!(PORT, 3000);
        }
    }

    #[test]
    fn override_port_matches_env() {
        if let Some(raw) = option_env!("PORT") {
            let expected: u16 = raw.parse().expect("PORT must be a valid u16");
            assert_eq!(PORT, expected,
                "PORT constant must match PORT env var");
        }
    }

    // ── WORKERS ───────────────────────────────────────────────────

    #[test]
    fn num_workers_default_is_reasonable() {
        if option_env!("WORKERS").is_none() {
            assert!(NUM_WORKERS >= 1,
                "expected at least 1 worker, got {NUM_WORKERS}");
            assert!(NUM_WORKERS <= 1024,
                "expected at most 1024 workers, got {NUM_WORKERS}");
        }
    }

    #[test]
    fn override_workers_matches_env() {
        if let Some(raw) = option_env!("WORKERS") {
            let expected: usize = raw.parse().expect("WORKERS must be a valid usize");
            assert_eq!(NUM_WORKERS, expected,
                "NUM_WORKERS constant must match WORKERS env var");
        }
    }

    // ── MAX_CONNS ─────────────────────────────────────────────────

    #[test]
    fn max_connections_default_is_4096() {
        if option_env!("MAX_CONNS").is_none() {
            assert_eq!(MAX_CONNECTIONS, 4096);
        }
    }

    #[test]
    fn override_max_connections_matches_env() {
        if let Some(raw) = option_env!("MAX_CONNS") {
            let expected: usize = raw.parse().expect("MAX_CONNS must be a valid usize");
            assert_eq!(MAX_CONNECTIONS, expected,
                "MAX_CONNECTIONS constant must match MAX_CONNS env var");
        }
    }

    // ── TCP_HANDLERS_PER_WORKER ────────────────────────────────────

    #[test]
    fn tcp_handlers_per_worker_default_is_formula() {
        if option_env!("TCP_HANDLERS_PER_WORKER").is_none() {
            let expected = (MAX_CONNECTIONS / NUM_WORKERS).max(64);
            assert_eq!(TCP_HANDLERS_PER_WORKER, expected,
                "TCP_HANDLERS_PER_WORKER should be max(MAX_CONNECTIONS / NUM_WORKERS, 64)");
        }
    }

    #[test]
    fn override_tcp_handlers_per_worker_matches_env() {
        if let Some(raw) = option_env!("TCP_HANDLERS_PER_WORKER") {
            let expected: usize =
                raw.parse().expect("TCP_HANDLERS_PER_WORKER must be a valid usize");
            assert_eq!(TCP_HANDLERS_PER_WORKER, expected,
                "TCP_HANDLERS_PER_WORKER constant must match TCP_HANDLERS_PER_WORKER env var");
        }
    }

    // ── H3_HANDLERS_PER_CONNECTION ─────────────────────────────────

    #[test]
    fn h3_handlers_per_connection_default_is_8() {
        if option_env!("H3_HANDLERS_PER_CONNECTION").is_none() {
            assert_eq!(H3_HANDLERS_PER_CONNECTION, 8);
        }
    }

    #[test]
    fn override_h3_handlers_per_connection_matches_env() {
        if let Some(raw) = option_env!("H3_HANDLERS_PER_CONNECTION") {
            let expected: usize =
                raw.parse().expect("H3_HANDLERS_PER_CONNECTION must be a valid usize");
            assert_eq!(H3_HANDLERS_PER_CONNECTION, expected,
                "H3_HANDLERS_PER_CONNECTION constant must match H3_HANDLERS_PER_CONNECTION env var");
        }
    }

    // ── SHUTDOWN_TIMEOUT_SECS ──────────────────────────────────────

    #[test]
    fn shutdown_timeout_default_is_3() {
        if option_env!("SHUTDOWN_TIMEOUT_SECS").is_none() {
            assert_eq!(SHUTDOWN_TIMEOUT_SECS, 3);
        }
    }

    #[test]
    fn override_shutdown_timeout_matches_env() {
        if let Some(raw) = option_env!("SHUTDOWN_TIMEOUT_SECS") {
            let expected: u64 =
                raw.parse().expect("SHUTDOWN_TIMEOUT_SECS must be a valid u64");
            assert_eq!(SHUTDOWN_TIMEOUT_SECS, expected,
                "SHUTDOWN_TIMEOUT_SECS constant must match SHUTDOWN_TIMEOUT_SECS env var");
        }
    }

    // ── H2_CONN_WINDOW ────────────────────────────────────────────

    #[test]
    fn h2_conn_window_default_is_16_mib() {
        if option_env!("H2_CONN_WINDOW").is_none() {
            assert_eq!(H2_CONN_WINDOW, 16 * 1024 * 1024);
        }
    }

    #[test]
    fn override_h2_conn_window_matches_env() {
        if let Some(raw) = option_env!("H2_CONN_WINDOW") {
            let expected: u32 = raw.parse().expect("H2_CONN_WINDOW must be a valid u32");
            assert_eq!(H2_CONN_WINDOW, expected,
                "H2_CONN_WINDOW constant must match H2_CONN_WINDOW env var");
        }
    }

    // ── H2_STREAM_WINDOW ──────────────────────────────────────────

    #[test]
    fn h2_stream_window_default_is_4_mib() {
        if option_env!("H2_STREAM_WINDOW").is_none() {
            assert_eq!(H2_STREAM_WINDOW, 4 * 1024 * 1024);
        }
    }

    #[test]
    fn override_h2_stream_window_matches_env() {
        if let Some(raw) = option_env!("H2_STREAM_WINDOW") {
            let expected: u32 = raw.parse().expect("H2_STREAM_WINDOW must be a valid u32");
            assert_eq!(H2_STREAM_WINDOW, expected,
                "H2_STREAM_WINDOW constant must match H2_STREAM_WINDOW env var");
        }
    }

    // ── H2_MAX_FRAME_SIZE ─────────────────────────────────────────

    #[test]
    fn h2_max_frame_size_default_is_65535() {
        if option_env!("H2_MAX_FRAME_SIZE").is_none() {
            assert_eq!(H2_MAX_FRAME_SIZE, 65_535);
        }
    }

    #[test]
    fn override_h2_max_frame_size_matches_env() {
        if let Some(raw) = option_env!("H2_MAX_FRAME_SIZE") {
            let expected: u32 = raw.parse().expect("H2_MAX_FRAME_SIZE must be a valid u32");
            assert_eq!(H2_MAX_FRAME_SIZE, expected,
                "H2_MAX_FRAME_SIZE constant must match H2_MAX_FRAME_SIZE env var");
        }
    }

    // ── H2_MAX_SEND_BUF ───────────────────────────────────────────

    #[test]
    fn h2_max_send_buf_default_is_256_kib() {
        if option_env!("H2_MAX_SEND_BUF").is_none() {
            assert_eq!(H2_MAX_SEND_BUF, 256 * 1024);
        }
    }

    #[test]
    fn override_h2_max_send_buf_matches_env() {
        if let Some(raw) = option_env!("H2_MAX_SEND_BUF") {
            let expected: usize = raw.parse().expect("H2_MAX_SEND_BUF must be a valid usize");
            assert_eq!(H2_MAX_SEND_BUF, expected,
                "H2_MAX_SEND_BUF constant must match H2_MAX_SEND_BUF env var");
        }
    }

    // ── DISABLE_LOGGING ───────────────────────────────────────────

    #[test]
    fn disable_logging_default_is_true() {
        if option_env!("DISABLE_LOGGING").is_none() {
            assert!(DISABLE_LOGGING);
        }
    }

    #[test]
    fn override_disable_logging_matches_env() {
        if let Some(raw) = option_env!("DISABLE_LOGGING") {
            let expected = raw == "1" || raw.to_lowercase() == "true";
            assert_eq!(DISABLE_LOGGING, expected,
                "DISABLE_LOGGING constant must match DISABLE_LOGGING env var");
        }
    }
}
