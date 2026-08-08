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
    usize,
    bool,
    Vec<bool>,
    Vec<bool>,
    Vec<bool>,
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
    let mut use_uncompressed: Vec<bool> = Vec::new();
    let mut use_brotli: Vec<bool> = Vec::new();
    let mut use_zstd: Vec<bool> = Vec::new();
    let mut has_404 = false;
    let mut not_found_const_prefix: Option<String> = None;
    let mut max_path_len: usize = 0;
    let mut max_size: usize = 0;
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
        // Choose the smallest among uncompressed, gzip, brotli, and zstd.
        let use_uncomp = uncompressed_len < gz_data.len()
            && uncompressed_len < br_data.len()
            && uncompressed_len < zst_data.len();
        let use_zst = !use_uncomp
            && zst_data.len() < gz_data.len()
            && zst_data.len() < br_data.len();
        let use_br = !use_uncomp && !use_zst && br_data.len() < gz_data.len();
        use_uncompressed.push(use_uncomp);
        use_brotli.push(use_br);
        use_zstd.push(use_zst);

        gzip_lengths.push(gz_data.len());
        brotli_lengths.push(br_data.len());
        zstd_lengths.push(zst_data.len());

        let (body_data, content_length) = if use_uncomp {
            let raw_path = format!("{gz_path}.raw");
            let raw_data = fs::read(&raw_path).expect("failed to read raw file");
            let len = raw_data.len();
            (raw_data, len)
        } else if use_zst {
            let len = zst_data.len();
            (zst_data, len)
        } else if use_br {
            let len = br_data.len();
            (br_data, len)
        } else {
            let len = gz_data.len();
            (gz_data, len)
        };
        max_size = max_size.max(content_length);
        uncompressed_lengths.push(uncompressed_len);

        // Per-file CSP: every directive is gated on actual page usage.
        let csp_value = csp::build_csp(file, csp_values);

        // Build header set for this asset
        let mut headers: Vec<(String, String)> = Vec::new();
        headers.push(("content-type".into(), content_type.into()));
        if use_zst {
            headers.push(("content-encoding".into(), "zstd".into()));
        } else if use_br {
            headers.push(("content-encoding".into(), "br".into()));
        } else if !use_uncomp {
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
            status_code,
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
        use_brotli,
        use_zstd,
        not_found_const_prefix,
        uncompressed_lengths,
        gzip_lengths,
        brotli_lengths,
        zstd_lengths,
    )
}
