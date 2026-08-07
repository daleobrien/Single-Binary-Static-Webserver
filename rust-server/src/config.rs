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
}
