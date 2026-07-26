/// Return all security headers that do NOT include the CSP.
/// The CSP is built per-file in `build_csp` so it reflects actual page usage.
pub(super) fn build_non_csp_headers() -> Vec<(String, String)> {
    vec![
        ("x-content-type-options".into(), "nosniff".into()),
        ("x-frame-options".into(), "DENY".into()),
        ("x-xss-protection".into(), "1; mode=block".into()),
        (
            "referrer-policy".into(),
            "strict-origin-when-cross-origin".into(),
        ),
        (
            "strict-transport-security".into(),
            "max-age=31536000; includeSubDomains".into(),
        ),
        (
            "permissions-policy".into(),
            "camera=(), microphone=(), geolocation=()".into(),
        ),
        ("alt-svc".into(), "h3=\\\":3000\\\"".into()),
    ]
}
