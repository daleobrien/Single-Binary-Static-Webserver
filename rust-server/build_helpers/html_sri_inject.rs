use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::build_helpers::processing;
use crate::build_helpers::utils;

/// Process HTML: inject the version-check script, replace JS/CSS filenames with
/// content-hashed versions plus SRI integrity attributes, then minify and gzip.
pub(super) fn update_html_sri_and_inject_update_js(
    files: &[String],
    file_hashes: &mut HashMap<String, String>,
    hashed_filenames: &HashMap<String, String>,
    version_script_tag: &str,
    gzip_dir: &str,
    uncompressed_lens: &mut HashMap<String, usize>,
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

        // Replace original filenames with content-hashed versions and
        // inject integrity="sha256-…" crossorigin="anonymous" on <link>/<script>/<img> tags.
        // Try both href (for <link>) and src (for <script>/<img>) — only the one
        // present in the HTML will match.
        for (asset_file, hash) in file_hashes.iter() {
            let hashed_name = hashed_filenames
                .get(asset_file)
                .map(|s| s.as_str())
                .unwrap_or(asset_file);
            for attr in &["href", "src"] {
                let pattern = format!("{attr}=\"/{asset_file}\"");
                let replacement =
                    format!("{attr}=\"/{hashed_name}\" integrity=\"sha256-{hash}\" crossorigin=\"anonymous\"");
                raw_str = raw_str.replace(&pattern, &replacement);
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
