use std::collections::HashMap;
use std::fs;

use crate::build_helpers::utils;

pub(super) fn build_version_headers(
    build_version: &str,
    gzip_dir: &str,
    br_dir: &str,
    zst_dir: &str,
    mut header_sets: Vec<Vec<(String, String)>>,
) -> (usize, Vec<Vec<(String, String)>>, usize, usize, usize, usize) {
    let version_body = build_version.as_bytes().to_vec();
    let version_gz_path = format!("{gzip_dir}/v.txt.gz");
    utils::compress_to_gzip(&version_body, &version_gz_path);
    let version_br_path = format!("{br_dir}/v.txt.br");
    utils::compress_to_brotli(&version_body, &version_br_path);
    let version_zst_path = format!("{zst_dir}/v.txt.zst");
    utils::compress_to_zstd(&version_body, &version_zst_path);
    let version_gz_data = fs::read(&version_gz_path).expect("failed to read version gzip");
    let version_br_data = fs::read(&version_br_path).expect("failed to read version brotli");
    let version_zst_data = fs::read(&version_zst_path).expect("failed to read version zstd");

    let version_uncompressed_len = version_body.len();
    let version_gzip_len = version_gz_data.len();
    let version_brotli_len = version_br_data.len();
    let version_zstd_len = version_zst_data.len();

    let mut version_headers: Vec<(String, String)> = Vec::new();
    version_headers.push(("content-type".into(), "text/plain; charset=utf-8".into()));
    // Content-Encoding is set dynamically at request time based on
    // Accept-Encoding negotiation.
    version_headers.push((
        "cache-control".into(),
        "no-cache, no-store, must-revalidate".into(),
    ));

    // ETag: the build version, used for conditional requests (If-None-Match → 304)
    version_headers.push(("etag".into(), build_version.to_string()));

    let version_header_key = utils::header_set_key(&version_headers);
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    // Rebuild index from existing header_sets
    for (i, set) in header_sets.iter().enumerate() {
        header_set_index.insert(utils::header_set_key(set), i);
    }

    let version_header_idx = *header_set_index
        .entry(version_header_key)
        .or_insert_with(|| {
            let i = header_sets.len();
            header_sets.push(version_headers);
            i
        });

    (
        version_header_idx,
        header_sets,
        version_uncompressed_len,
        version_gzip_len,
        version_brotli_len,
        version_zstd_len,
    )
}
