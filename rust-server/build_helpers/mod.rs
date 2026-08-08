mod asset_metadata;
mod codegen;
mod collect;
mod config_gen;
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
use crate::build_helpers::codegen::{generate, CodegenCtx};
use crate::build_helpers::collect::collect_source_files;
use crate::build_helpers::html_sri_inject::update_html_sri_and_inject_update_js;
use crate::build_helpers::minify_hash_compress::minify_compute_sha_and_compress;
use crate::build_helpers::non_csp_headers::build_non_csp_headers;
use crate::build_helpers::not_found_headers::build_not_found_headers;
use crate::build_helpers::version_hash::compute_version_hash;
use crate::build_helpers::version_headers::build_version_headers;
use crate::build_helpers::version_script::build_version_script;

pub fn run() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let gzip_dir = format!("{out_dir}/gzip");
    let br_dir = format!("{out_dir}/brotli");
    let zst_dir = format!("{out_dir}/zstd");

    // Clean and recreate the gzip output directory
    let _ = std::fs::remove_dir_all(&gzip_dir);
    std::fs::create_dir_all(&gzip_dir).expect("failed to create gzip dir");

    // Clean and recreate the brotli output directory
    let _ = std::fs::remove_dir_all(&br_dir);
    std::fs::create_dir_all(&br_dir).expect("failed to create brotli dir");

    // Clean and recreate the zstd output directory
    let _ = std::fs::remove_dir_all(&zst_dir);
    std::fs::create_dir_all(&zst_dir).expect("failed to create zstd dir");

    // ── Generate server configuration constants ──
    config_gen::generate(&out_dir);

    // ── TLS certificate handling ──
    tls::setup_tls(&out_dir);

    // ── Collect all source files ──
    let files = collect_source_files();

    // ── Compute version hash from build timestamp ──
    let build_version = compute_version_hash();

    // ── Pre-build the version-check script and its CSP hash ──
    let (version_script_tag, csp_script_hash) = build_version_script(&build_version);

    // ── Read DISABLE_SRI env var ──
    let disable_sri: bool = std::env::var("DISABLE_SRI")
        .ok()
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(false);

    // ── Read ALLOW_INLINE_STYLES env var ──
    let allow_inline_styles: bool = std::env::var("ALLOW_INLINE_STYLES")
        .ok()
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(false);

    // ── Two-pass file processing ──
    let mut uncompressed_lens: HashMap<String, usize> = HashMap::new();
    let mut original_lens: HashMap<String, usize> = HashMap::new();
    let mut gzip_lens: HashMap<String, usize> = HashMap::new();
    let mut br_lens: HashMap<String, usize> = HashMap::new();
    let mut zst_lens: HashMap<String, usize> = HashMap::new();
    let mut file_hashes = minify_compute_sha_and_compress(
        &files,
        &gzip_dir,
        &br_dir,
        &zst_dir,
        &mut uncompressed_lens,
        &mut original_lens,
        &mut gzip_lens,
        &mut br_lens,
        &mut zst_lens,
    );
    update_html_sri_and_inject_update_js(
        &files,
        &mut file_hashes,
        &version_script_tag,
        &gzip_dir,
        &br_dir,
        &zst_dir,
        &mut uncompressed_lens,
        &mut original_lens,
        &mut gzip_lens,
        &mut br_lens,
        &mut zst_lens,
        disable_sri,
    );

    // ── Print build summary ──
    print_build_summary(&files, &original_lens, &uncompressed_lens, &gzip_lens, &br_lens, &zst_lens);

    // ── Security headers (CSP is built per-file in build_asset_metadata) ──
    let security_headers = build_non_csp_headers();

    // ── Pre-compute CSP directive values for reuse across both regular
    //     asset metadata and the 404 fallback header set. ──
    let csp_values = csp::build_csp_values(&file_hashes, &csp_script_hash, disable_sri, allow_inline_styles);

    // ── Configurable 404 filename (env var with default) ──
    let not_found_filename =
        std::option_env!("NOT_FOUND_FILENAME").unwrap_or("404.html");

    // ── Build asset metadata and header deduplication ──
    let (
        assets,
        asset_header_indices,
        header_sets,
        _max_path_len,
        has_404,
        not_found_const_prefix,
        uncompressed_lengths,
        gzip_lengths,
        brotli_lengths,
        zstd_lengths,
    ) = build_asset_metadata(
        &files,
        &gzip_dir,
        &br_dir,
        &zst_dir,
        &security_headers,
        &file_hashes,
        &csp_values,
        &uncompressed_lens,
        &build_version,
        not_found_filename,
    );

    // ── Version asset ──
    let (version_header_idx, mut header_sets, version_uncompressed_len, version_gzip_len, version_brotli_len, version_zstd_len) =
        build_version_headers(&build_version, &gzip_dir, &br_dir, &zst_dir, header_sets);

    let not_found_header_idx = if !has_404 {
        let (idx, hs) = build_not_found_headers(&security_headers, header_sets, &build_version);
        header_sets = hs;
        idx
    } else {
        // When 404.html exists, it uses the regular asset headers — no separate
        // header set is needed.
        0
    };

    // ── Generate Rust source ──
    let ctx = CodegenCtx {
        out_dir,
        build_version,
        assets,
        asset_header_indices,
        header_sets,
        version_header_idx,
        not_found_header_idx,
        not_found_const_prefix,
        files,
        has_404,
        uncompressed_lengths,
        version_uncompressed_len,
        gzip_lengths,
        brotli_lengths,
        zstd_lengths,
        version_gzip_len,
        version_brotli_len,
        version_zstd_len,
    };
    generate(&ctx);
}

/// Print a summary table of each processed file showing original size, minified
/// size, gzip size, brotli size, zstd size, and overall compression ratio.
fn print_build_summary(
    files: &[String],
    original_lens: &HashMap<String, usize>,
    uncompressed_lens: &HashMap<String, usize>,
    gzip_lens: &HashMap<String, usize>,
    br_lens: &HashMap<String, usize>,
    zst_lens: &HashMap<String, usize>,
) {
    let mut entries: Vec<(&String, usize, usize, usize, usize, usize)> = files
        .iter()
        .filter_map(|f| {
            let orig = *original_lens.get(f)?;
            let uncomp = *uncompressed_lens.get(f)?;
            let gz = *gzip_lens.get(f)?;
            let br = *br_lens.get(f)?;
            let zst = *zst_lens.get(f)?;
            Some((f, orig, uncomp, gz, br, zst))
        })
        .collect();

    if entries.is_empty() {
        return;
    }

    // Sort by original size descending for readability.
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    // Compute totals.
    let total_orig: usize = entries.iter().map(|e| e.1).sum();
    let total_gz: usize = entries.iter().map(|e| e.3).sum();
    let total_br: usize = entries.iter().map(|e| e.4).sum();
    let total_zst: usize = entries.iter().map(|e| e.5).sum();

    // Determine best encoding totals (what will actually be served).
    let total_best: usize = entries
        .iter()
        .map(|(_, _, uncomp, gz, br, zst)| {
            if *uncomp <= *gz && *uncomp <= *br && *uncomp <= *zst {
                *uncomp
            } else if *zst <= *gz && *zst <= *br {
                *zst
            } else if *br <= *gz {
                *br
            } else {
                *gz
            }
        })
        .sum();

    println!("\n========== Build Summary ==========");
    println!(
        "{:<40} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "File", "Original", "Minified", "Gzip", "Brotli", "Zstd", "Best"
    );
    println!("{:-<106}", "");

    for (file, orig, uncomp, gz, br, zst) in &entries {
        let (best, best_label) = if *uncomp <= *gz && *uncomp <= *br && *uncomp <= *zst {
            (*uncomp, "")
        } else if *zst <= *gz && *zst <= *br {
            (*zst, "zst")
        } else if *br <= *gz {
            (*br, "br")
        } else {
            (*gz, "gz")
        };
        println!(
            "{:<40} {:>10} {:>10} {:>10} {:>10} {:>10} {:>9} {}",
            truncate_str(file, 40),
            format_bytes(*orig),
            format_bytes(*uncomp),
            format_bytes(*gz),
            format_bytes(*br),
            format_bytes(*zst),
            format_bytes(best),
            best_label
        );
    }

    println!("{:-<106}", "");
    let overall_ratio = if total_orig > 0 {
        (total_best as f64 / total_orig as f64) * 100.0
    } else {
        100.0
    };
    println!(
        "{:<40} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "TOTAL",
        format_bytes(total_orig),
        "",
        format_bytes(total_gz),
        format_bytes(total_br),
        format_bytes(total_zst),
        format_bytes(total_best)
    );
    println!(
        "Overall compression: {:.1}% of original",
        overall_ratio
    );
    println!("====================================\n");
}

/// Format a byte count as a human-readable string (e.g. "1.2 KB").
fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit_idx])
    }
}

/// Truncate a string to `max_len` characters, appending "…" if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
