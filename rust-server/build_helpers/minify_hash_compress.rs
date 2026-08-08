use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::build_helpers::processing;
use crate::build_helpers::utils;

/// Minify CSS/JS, compute SHA-256 hashes, and compress non-HTML assets
/// with gzip, brotli, and zstd. Returns file_hashes: base64 SHA-256.
pub(super) fn minify_compute_sha_and_compress(
    files: &[String],
    gzip_dir: &str,
    br_dir: &str,
    zst_dir: &str,
    uncompressed_lens: &mut HashMap<String, usize>,
    original_lens: &mut HashMap<String, usize>,
    gzip_lens: &mut HashMap<String, usize>,
    br_lens: &mut HashMap<String, usize>,
    zst_lens: &mut HashMap<String, usize>,
) -> HashMap<String, String> {
    let mut file_hashes: HashMap<String, String> = HashMap::new();

    for file in files {
        let ext = Path::new(file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext == "html" {
            continue;
        }
        let input_path = format!("../public/{file}");
        let raw = fs::read(&input_path).expect("failed to read source file");

        // Track original (pre-minification) size.
        original_lens.insert(file.clone(), raw.len());

        let minified = processing::minify_file(file, &raw);

        // Compute SHA-256 of the (uncompressed) body for the Digest header.
        file_hashes.insert(file.clone(), utils::sha256_base64(&minified));

        // Track uncompressed length for later size comparison.
        uncompressed_lens.insert(file.clone(), minified.len());

        let gz_len =
            utils::compress_to_gzip(&minified, &format!("{gzip_dir}/{file}.gz"));
        gzip_lens.insert(file.clone(), gz_len);

        let br_len =
            utils::compress_to_brotli(&minified, &format!("{br_dir}/{file}.br"));
        br_lens.insert(file.clone(), br_len);

        let zst_len =
            utils::compress_to_zstd(&minified, &format!("{zst_dir}/{file}.zst"));
        zst_lens.insert(file.clone(), zst_len);
    }

    file_hashes
}
