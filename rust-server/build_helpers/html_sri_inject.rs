use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::build_helpers::processing;
use crate::build_helpers::utils;

/// Process HTML: inject the version-check script, add SRI integrity attributes
/// to <link>/<script>/<img> tags (keeping original filenames), then minify and gzip.
pub(super) fn update_html_sri_and_inject_update_js(
    files: &[String],
    file_hashes: &mut HashMap<String, String>,
    version_script_tag: &str,
    gzip_dir: &str,
    uncompressed_lens: &mut HashMap<String, usize>,
    disable_sri: bool,
) {
    for file in files {
        if Path::new(file).extension().and_then(|e| e.to_str()) != Some("html") {
            continue;
        }
        let input_path = format!("../public/{file}");
        let mut raw = fs::read(&input_path).expect("failed to read source file");
        let mut raw_str = String::from_utf8_lossy(&raw).to_string();

        // Inject the pre-minified, pre-hashed version-check script before </body>.
        if let Some(pos) = raw_str.rfind("</body>") {
            let mut injected = raw_str[..pos].to_string();
            injected.push_str(version_script_tag);
            injected.push_str(&raw_str[pos..]);
            raw_str = injected;
        }

        // Inject integrity="sha256-…" crossorigin="anonymous" on <link>/<script>/<img> tags.
        // Filenames are kept as-is (no content-hash renaming).
        // Try both href (for <link>) and src (for <script>/<img>) — only the one
        // present in the HTML will match.
        // When SRI is disabled, skip this step entirely.
        if !disable_sri {
            for (asset_file, hash) in file_hashes.iter() {
                for attr in &["href", "src"] {
                    let pattern = format!("{attr}=\"/{asset_file}\"");
                    let replacement =
                        format!("{attr}=\"/{asset_file}\" integrity=\"sha256-{hash}\" crossorigin=\"anonymous\"");
                    raw_str = raw_str.replace(&pattern, &replacement);
                }
            }
        }

        raw = raw_str.into_bytes();

        // Minify with minify_js: false — the injected inline JS is already minified.
        let minified = minify_html::minify(&raw, &processing::html_cfg(false));

        // Compute SHA-256 of the final (uncompressed) HTML for the Digest header.
        file_hashes.insert(file.clone(), utils::sha256_base64(&minified));

        // Track uncompressed length for later size comparison.
        uncompressed_lens.insert(file.clone(), minified.len());

        utils::compress_to_gzip(&minified, &format!("{gzip_dir}/{file}.gz"));
    }
}
