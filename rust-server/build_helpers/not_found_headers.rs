use std::collections::HashMap;

use crate::build_helpers::utils;

/// Build the header set for the inline 404 fallback.
/// Only called when there is no real 404.html in public/.
pub(super) fn build_not_found_headers(
    security_headers: &[(String, String)],
    mut header_sets: Vec<Vec<(String, String)>>,
    build_version: &str,
) -> (usize, Vec<Vec<(String, String)>>) {
    let body: &[u8] = b"<h1>404 - Not Found</h1>";
    let hash = utils::sha256_base64(body);
    let cl = body.len().to_string();

    let mut not_found_headers: Vec<(String, String)> = Vec::new();
    not_found_headers.push(("content-type".into(), "text/html; charset=utf-8".into()));
    not_found_headers.push(("content-length".into(), cl.clone()));

    // Locked-down CSP for the inline fallback (no external resources).
    let csp = format!(
        "default-src 'none'; script-src 'none'; style-src 'none'; img-src 'none'; font-src 'none'; media-src 'none'; frame-src 'none'; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
    );
    not_found_headers.push(("content-security-policy".into(), csp));
    not_found_headers.extend_from_slice(security_headers);
    not_found_headers.push(("cache-control".into(), "public, max-age=3600".into()));
    not_found_headers.push(("etag".into(), build_version.to_string()));
    not_found_headers.push(("repr-digest".into(), format!("sha-256={}", hash)));
    not_found_headers.push(("content-digest".into(), format!("sha-256={}", hash)));

    // Rebuild index from existing header_sets
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    for (i, set) in header_sets.iter().enumerate() {
        header_set_index.insert(utils::header_set_key(set), i);
    }

    let key = utils::header_set_key(&not_found_headers);
    let idx = *header_set_index.entry(key).or_insert_with(|| {
        let i = header_sets.len();
        header_sets.push(not_found_headers);
        i
    });

    (idx, header_sets)
}
