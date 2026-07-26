use std::collections::HashMap;
use std::fs;

use crate::build_helpers::utils;

pub(super) fn build_version_headers(
    build_version: &str,
    gzip_dir: &str,
    mut header_sets: Vec<Vec<(String, String)>>,
) -> (usize, usize, bool, Vec<Vec<(String, String)>>) {
    let version_body = build_version.as_bytes().to_vec();
    let version_gz_path = format!("{gzip_dir}/v.txt.gz");
    utils::compress_to_gzip(&version_body, &version_gz_path);
    let version_gz_data =
        fs::read(&version_gz_path).expect("failed to read version gzip");
    let version_use_uncomp = version_body.len() < version_gz_data.len();
    let version_len = if version_use_uncomp {
        version_body.len()
    } else {
        version_gz_data.len()
    };

    let mut version_headers: Vec<(String, String)> = Vec::new();
    version_headers.push(("content-type".into(), "text/plain; charset=utf-8".into()));
    if !version_use_uncomp {
        version_headers.push(("content-encoding".into(), "gzip".into()));
    }
    version_headers.push((
        "cache-control".into(),
        "no-cache, no-store, must-revalidate".into(),
    ));

    // ETag: the build version, used for conditional requests (If-None-Match → 304)
    version_headers.push((
        "etag".into(),
        build_version.to_string(),
    ));
    // Content-Digest: SHA-256 of the body actually sent
    let content_digest_data = if version_use_uncomp {
        &version_body
    } else {
        &version_gz_data
    };
    version_headers.push((
        "content-digest".into(),
        format!("sha-256={}", utils::sha256_base64(content_digest_data)),
    ));

    let version_header_key = utils::header_set_key(&version_headers);
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    // Rebuild index from existing header_sets
    for (i, set) in header_sets.iter().enumerate() {
        header_set_index.insert(utils::header_set_key(set), i);
    }

    let version_header_idx =
        *header_set_index
            .entry(version_header_key)
            .or_insert_with(|| {
                let i = header_sets.len();
                header_sets.push(version_headers);
                i
            });

    (version_header_idx, version_len, version_use_uncomp, header_sets)
}
