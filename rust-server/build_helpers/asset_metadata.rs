use std::collections::HashMap;
use std::fs;

use crate::build_helpers::codegen::AssetGen;
use crate::build_helpers::csp;
use crate::build_helpers::utils;

pub(super) fn build_asset_metadata(
    files: &[String],
    gzip_dir: &str,
    br_dir: &str,
    zst_dir: &str,
    security_headers: &[(String, String)],
    file_hashes: &HashMap<String, String>,
    csp_values: &csp::CspValues,
    uncompressed_lens: &HashMap<String, usize>,
    build_version: &str,
    not_found_filename: &str,
) -> (
    Vec<AssetGen>,
    Vec<usize>,
    Vec<Vec<(String, String)>>,
    usize,
    bool,
    Option<String>,
    Vec<usize>,
    Vec<usize>,
    Vec<usize>,
    Vec<usize>,
) {
    let mut header_sets: Vec<Vec<(String, String)>> = Vec::new();
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    let mut assets: Vec<AssetGen> = Vec::new();
    let mut asset_header_indices: Vec<usize> = Vec::new();
    let mut has_404 = false;
    let mut not_found_const_prefix: Option<String> = None;
    let mut max_path_len: usize = 0;
    let mut uncompressed_lengths: Vec<usize> = Vec::with_capacity(files.len());
    let mut gzip_lengths: Vec<usize> = Vec::with_capacity(files.len());
    let mut brotli_lengths: Vec<usize> = Vec::with_capacity(files.len());
    let mut zstd_lengths: Vec<usize> = Vec::with_capacity(files.len());

    for file in files {
        let content_type = utils::mime_for_file(file);
        let const_prefix = utils::file_to_const(file);
        let url_paths = utils::url_paths_for_file(file);

        for path in &url_paths {
            max_path_len = max_path_len.max(path.len());
        }

        let status_code = if file == not_found_filename {
            has_404 = true;
            not_found_const_prefix = Some(const_prefix.clone());
            404
        } else {
            200
        };

        let gz_name = format!("{file}.gz");
        let gz_path = format!("{gzip_dir}/{gz_name}");
        let gz_data = fs::read(&gz_path).expect("failed to read gzipped file");
        let br_name = format!("{file}.br");
        let br_path = format!("{br_dir}/{br_name}");
        let br_data = fs::read(&br_path).expect("failed to read brotli file");
        let zst_name = format!("{file}.zst");
        let zst_path = format!("{zst_dir}/{zst_name}");
        let zst_data = fs::read(&zst_path).expect("failed to read zstd file");

        let uncompressed_len = uncompressed_lens
            .get(file)
            .copied()
            .unwrap_or(gz_data.len());

        gzip_lengths.push(gz_data.len());
        brotli_lengths.push(br_data.len());
        zstd_lengths.push(zst_data.len());
        uncompressed_lengths.push(uncompressed_len);

        // Per-file CSP: every directive is gated on actual page usage.
        let csp_value = csp::build_csp(file, csp_values);

        // Build header set for this asset.
        // Content-Encoding is set dynamically at request time based on
        // Accept-Encoding negotiation, so it is not part of the static headers.
        let mut headers: Vec<(String, String)> = Vec::new();
        headers.push(("content-type".into(), content_type.into()));
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
            status_code,
        });
    }

    (
        assets,
        asset_header_indices,
        header_sets,
        max_path_len,
        has_404,
        not_found_const_prefix,
        uncompressed_lengths,
        gzip_lengths,
        brotli_lengths,
        zstd_lengths,
    )
}
