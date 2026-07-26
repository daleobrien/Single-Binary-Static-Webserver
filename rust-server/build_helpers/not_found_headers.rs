use std::collections::HashMap;
use std::fs;

use crate::build_helpers::csp;
use crate::build_helpers::utils;

pub(super) fn build_not_found_headers(
    has_404: bool,
    security_headers: &[(String, String)],
    file_hashes: &HashMap<String, String>,
    gzip_dir: &str,
    uncompressed_lens: &HashMap<String, usize>,
    mut header_sets: Vec<Vec<(String, String)>>,
    build_version: &str,
    csp_values: &csp::CspValues,
) -> (usize, bool, Vec<Vec<(String, String)>>) {
    let mut not_found_headers: Vec<(String, String)> = Vec::new();
    not_found_headers.push(("content-type".into(), "text/html; charset=utf-8".into()));
    let mut not_found_use_uncomp = false;
    let gz_404 = if has_404 {
        let gz = fs::read(format!("{gzip_dir}/404.html.gz")).expect("failed to read 404 gzip");
        let orig_len = uncompressed_lens.get("404.html").copied().unwrap_or(0);
        not_found_use_uncomp = orig_len < gz.len();
        if !not_found_use_uncomp {
            not_found_headers.push(("content-encoding".into(), "gzip".into()));
        }
        Some(gz)
    } else {
        None
    };
    // Build CSP from the actual 404.html content so that external resources
    // like stylesheets and scripts referenced by the 404 page are not blocked.
    let csp_404 = csp::build_csp("404.html", csp_values);
    not_found_headers.push(("content-security-policy".into(), csp_404));
    not_found_headers.extend_from_slice(security_headers);
    not_found_headers.push(("cache-control".into(), "public, max-age=3600".into()));
    not_found_headers.push(("etag".into(), build_version.to_string()));

    // Rebuild index from existing header_sets
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    for (i, set) in header_sets.iter().enumerate() {
        header_set_index.insert(utils::header_set_key(set), i);
    }

    let not_found_header_idx = if has_404 {
        // Repr-Digest from the uncompressed 404 HTML
        if let Some(hash) = file_hashes.get("404.html") {
            not_found_headers.push(("repr-digest".into(), format!("sha-256={}", hash)));
        }
        // Content-Digest from the body actually sent
        let content_digest_data = if not_found_use_uncomp {
            let raw_path = format!("{gzip_dir}/404.html.gz.raw");
            fs::read(&raw_path).expect("failed to read 404 raw")
        } else {
            gz_404.expect("gz_404 must be Some when has_404 and not using uncompressed")
        };
        not_found_headers.push((
            "content-digest".into(),
            format!("sha-256={}", utils::sha256_base64(&content_digest_data)),
        ));

        let key = utils::header_set_key(&not_found_headers);
        *header_set_index.entry(key).or_insert_with(|| {
            let i = header_sets.len();
            header_sets.push(not_found_headers);
            i
        })
    } else {
        let body: &[u8] = b"<h1>404 - Not Found</h1>";
        let hash = utils::sha256_base64(body);
        let cl = body.len().to_string();
        let mut h = not_found_headers;
        h.push(("content-length".into(), cl.clone()));
        h.push(("repr-digest".into(), format!("sha-256={}", hash)));
        h.push(("content-digest".into(), format!("sha-256={}", hash)));
        let key = utils::header_set_key(&h);
        *header_set_index.entry(key).or_insert_with(|| {
            let i = header_sets.len();
            header_sets.push(h);
            i
        })
    };

    (not_found_header_idx, not_found_use_uncomp, header_sets)
}
