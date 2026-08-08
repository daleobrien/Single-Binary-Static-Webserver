/// Returns `true` when the error was caused by the client cancelling
/// (e.g. browser navigated away) — not a real server error worth logging.
///
/// Uses `Debug` formatting rather than `Display` because many error types
/// produce shorter, more stable debug representations. The `Display` output
/// is often human-readable prose that can change between dependency versions.
///
/// If the upstream crates expose structured error kinds in the future, this
/// should be migrated to type-based matching instead of string matching.
#[cfg(not(disable_http3))]
pub(crate) fn is_client_cancel(e: &dyn std::error::Error) -> bool {
    let msg = format!("{e:?}");
    // Case-insensitive substring matching for robustness against
    // dependency formatting changes (e.g., Display → Debug variants).
    // Each pattern is minimal and specific — avoid broad matches like
    // "cancel" alone which could match unrelated errors.
    msg.len() > 0
        && (msg.contains("H3_REQUEST_CANCELLED")
            || msg.contains("h3_request_cancelled")
            || msg.contains("request cancelled")
            || msg.contains("aborted by peer"))
}

#[cfg(test)]
#[cfg(not(disable_http3))]
mod tests {
    use super::*;

    // A simple error type for testing is_client_cancel.
    #[derive(Debug)]
    struct FakeError(String);

    impl std::fmt::Display for FakeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for FakeError {}

    #[test]
    fn client_cancel_detects_known_patterns() {
        assert!(is_client_cancel(&FakeError("H3_REQUEST_CANCELLED".into())));
        assert!(is_client_cancel(&FakeError("h3_request_cancelled".into())));
        assert!(is_client_cancel(&FakeError("request cancelled".into())));
        assert!(is_client_cancel(&FakeError("aborted by peer".into())));
    }

    #[test]
    fn client_cancel_rejects_normal_errors() {
        assert!(!is_client_cancel(&FakeError(
            "internal server error".into()
        )));
        assert!(!is_client_cancel(&FakeError("".into())));
        assert!(!is_client_cancel(&FakeError("h3_request".into())));
    }
}
