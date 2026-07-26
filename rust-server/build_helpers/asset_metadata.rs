use std::collections::HashMap;
use std::fs;

use crate::build_helpers::codegen::AssetGen;
use crate::build_helpers::csp;
use crate::build_helpers::utils;

pub(super) fn build_asset_metadata(
    files: &[String],
    gzip_dir: &str,
    security_headers: &[(String, String)],
    file_hashes: &HashMap<String, String>,
    csp_script_hash: &str,
    hashed_filenames: &HashMap<String, String>,
    uncompressed_lens: &HashMap<String, usize>,
    build_version: &str,
) -> (
    Vec<AssetGen>,
    Vec<usize>,
    Vec<Vec<(String, String)>>,
    usize,
    usize,
    bool,
    Vec<bool>,
) {
    let mut header_sets: Vec<Vec<(String, String)>> = Vec::new();
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    let mut assets: Vec<AssetGen> = Vec::new();
    let mut asset_header_indices: Vec<usize> = Vec::new();
    let mut use_uncompressed: Vec<bool> = Vec::new();
    let mut has_404 = false;
    let mut max_path_len: usize = 0;
    let mut max_size: usize = 0;

    // Pre-compute CSP directive values once rather than re-filtering per file.
    let csp_values = csp::build_csp_values(file_hashes, csp_script_hash);

    for file in files {
        let content_type = utils::mime_for_file(file);
        let const_prefix = utils::file_to_const(file);
        // Use the content-hashed filename for URL paths when available.
        let url_file = hashed_filenames
            .get(file)
            .map(|s| s.as_str())
            .unwrap_or(file);
        let url_paths = utils::url_paths_for_file(url_file);

        for path in &url_paths {
            max_path_len = max_path_len.max(path.len());
        }

        if file == "404.html" {
            has_404 = true;
        }

        let gz_name = format!("{file}.gz");
        let gz_path = format!("{gzip_dir}/{gz_name}");
        let gz_data = fs::read(&gz_path).expect("failed to read gzipped file");
        let uncompressed_len = uncompressed_lens.get(file).copied().unwrap_or(gz_data.len());
        let use_uncomp = uncompressed_len < gz_data.len();
        use_uncompressed.push(use_uncomp);

        let (body_data, content_length) = if use_uncomp {
            let raw_path = format!("{gz_path}.raw");
            let raw_data = fs::read(&raw_path).expect("failed to read raw file");
            let len = raw_data.len();
            (raw_data, len)
        } else {
            let len = gz_data.len();
            (gz_data, len)
        };
        max_size = max_size.max(content_length);

        // Per-file CSP: every directive is gated on actual page usage.
        let csp_value = csp::build_csp(file, &csp_values);

        // Build header set for this asset
        let mut headers: Vec<(String, String)> = Vec::new();
        headers.push(("content-type".into(), content_type.into()));
        if !use_uncomp {
            headers.push(("content-encoding".into(), "gzip".into()));
        }
        headers.push(("content-security-policy".into(), csp_value));
        headers.extend_from_slice(security_headers);

        // Cache-Control per file
        let cache_control = if content_type.starts_with("text/html") {
            "public, max-age=3600"
        } else {
            "public, max-age=31536000, immutable"
        };
        headers.push(("cache-control".into(), cache_control.into()));

        // Repr-Digest: SHA-256 of the uncompressed (minified) representation body
        if let Some(hash) = file_hashes.get(file) {
            headers.push(("repr-digest".into(), format!("sha-256={}", hash)));
        }

        // Content-Digest: SHA-256 of the actual bytes sent over the wire
        headers.push((
            "content-digest".into(),
            format!("sha-256={}", utils::sha256_base64(&body_data)),
        ));

        // ETag: the build version — allows conditional requests (If-None-Match → 304)
        // for every resource, not just the /v endpoint.
        headers.push(("etag".into(), build_version.to_string()));

        // Deduplicate: get or assign a builder index
        let key = utils::header_set_key(&headers);
        let idx = *header_set_index.entry(key).or_insert_with(|| {
            let i = header_sets.len();
            header_sets.push(headers);
            i
        });
        asset_header_indices.push(idx);

        assets.push(AssetGen {
            const_prefix,
            url_paths,
        });
    }

    (
        assets,
        asset_header_indices,
        header_sets,
        max_path_len,
        max_size,
        has_404,
        use_uncompressed,
    )
}
