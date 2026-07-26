mod asset_metadata;
mod codegen;
mod collect;
mod csp;
mod html_sri_inject;
mod minify_hash_compress;
mod non_csp_headers;
mod not_found_headers;
mod processing;
mod tls;
mod utils;
mod version_hash;
mod version_headers;
mod version_script;

use std::collections::HashMap;

use crate::build_helpers::asset_metadata::build_asset_metadata;
use crate::build_helpers::codegen::{CodegenCtx, generate};
use crate::build_helpers::collect::collect_source_files;
use crate::build_helpers::non_csp_headers::build_non_csp_headers;
use crate::build_helpers::not_found_headers::build_not_found_headers;
use crate::build_helpers::html_sri_inject::update_html_sri_and_inject_update_js;
use crate::build_helpers::minify_hash_compress::minify_compute_sha_and_compress;
use crate::build_helpers::version_hash::compute_version_hash;
use crate::build_helpers::version_headers::build_version_headers;
use crate::build_helpers::version_script::build_version_script;

pub fn run() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let gzip_dir = format!("{out_dir}/gzip");

    // Clean and recreate the gzip output directory
    let _ = std::fs::remove_dir_all(&gzip_dir);
    std::fs::create_dir_all(&gzip_dir).expect("failed to create gzip dir");

    // ── TLS certificate handling ──
    tls::setup_tls(&out_dir);

    // ── Collect all source files ──
    let files = collect_source_files();

    // ── Compute version hash from build timestamp ──
    let build_version = compute_version_hash();

    // ── Pre-build the version-check script and its CSP hash ──
    let (version_script_tag, csp_script_hash) =
        build_version_script(&build_version);

    // ── Two-pass file processing ──
    let mut uncompressed_lens: HashMap<String, usize> = HashMap::new();
    let (mut file_hashes, hashed_filenames) =
        minify_compute_sha_and_compress(&files, &gzip_dir, &mut uncompressed_lens);
    update_html_sri_and_inject_update_js(
        &files,
        &mut file_hashes,
        &hashed_filenames,
        &version_script_tag,
        &gzip_dir,
        &mut uncompressed_lens,
    );

    // ── Security headers (CSP is built per-file in build_asset_metadata) ──
    let security_headers = build_non_csp_headers();

    // ── Build asset metadata and header deduplication ──
    let (assets, asset_header_indices, header_sets, max_path_len, max_size, has_404, use_uncompressed) =
        build_asset_metadata(&files, &gzip_dir, &security_headers, &file_hashes, &csp_script_hash, &hashed_filenames, &uncompressed_lens, &build_version);

    // ── Version asset ──
    let (version_header_idx, version_len, version_use_uncompressed, header_sets) =
        build_version_headers(
            &build_version,
            &gzip_dir,
            header_sets,
        );

    // ── 404 header set ──
    let (not_found_header_idx, not_found_use_uncompressed, header_sets) =
        build_not_found_headers(has_404, &security_headers, &csp_script_hash, &file_hashes, &gzip_dir, &uncompressed_lens, header_sets, &build_version);

    // ── Generate Rust source ──
    let ctx = CodegenCtx {
        out_dir,
        gzip_dir,
        build_version,
        assets,
        asset_header_indices,
        header_sets,
        version_header_idx,
        version_len,
        not_found_header_idx,
        files,
        has_404,
        max_path_len,
        max_size,
        use_uncompressed,
        version_use_uncompressed,
        not_found_use_uncompressed,
    };
    generate(&ctx);
}
