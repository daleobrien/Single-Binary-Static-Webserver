/// Returns `true` when the error was caused by the client cancelling
/// (e.g. browser navigated away) — not a real server error worth logging.
pub(crate) fn is_client_cancel(e: &dyn std::error::Error) -> bool {
    let msg = e.to_string();
    msg.contains("H3_REQUEST_CANCELLED")
        || msg.contains("h3_request_cancelled")
        || msg.contains("request cancelled")
        || msg.contains("aborted by peer")
}

#[cfg(test)]
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
