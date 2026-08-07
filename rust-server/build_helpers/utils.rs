use std::fs;
use std::io::Write;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};

/// Map a filename's extension to its MIME type.
/// Text types include `charset=utf-8` so callers do not need to append it separately.
pub fn mime_for_file(filename: &str) -> &'static str {
    match Path::new(filename).extension().and_then(|e| e.to_str()) {
        // Text
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("svg") => "image/svg+xml",
        // Images
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("avif") => "image/avif",
        // Fonts
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        // Other
        Some("wasm") => "application/wasm",
        Some("pdf") => "application/pdf",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        // Audio
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    }
}

/// Convert a filename into a valid Rust const identifier prefix.
///
/// Example: `"script.js"` → `"SCRIPT_JS"`, `"404.html"` → `"F_404_HTML"`
pub fn file_to_const(filename: &str) -> String {
    let s = filename
        .replace('/', "_")
        .replace('.', "_")
        .replace('-', "_")
        .to_uppercase();
    // Rust identifiers cannot start with a digit
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("F_{s}")
    } else {
        s
    }
}

/// Compute the URL paths a file should be served under.
///
/// HTML files get an extensionless alias (and `index.html` additionally serves `/`).
pub fn url_paths_for_file(filename: &str) -> Vec<String> {
    let path = Path::new(filename);
    let stem = path.file_stem().unwrap().to_str().unwrap();
    let ext = path.extension().unwrap().to_str().unwrap();

    let mut paths = vec![format!("/{filename}")];

    // HTML files also get an extensionless alias
    if ext == "html" {
        if stem == "index" {
            paths.push("/".to_string());
        } else {
            paths.push(format!("/{stem}"));
        }
    }

    paths
}

/// Build a canonical string key for a set of headers (for deduplication).
///
/// Parts are sorted so that header order is irrelevant to the key.
pub fn header_set_key(headers: &[(String, String)]) -> String {
    let mut parts: Vec<String> = headers.iter().map(|(k, v)| format!("{k}:{v}")).collect();
    parts.sort();
    parts.join("\n")
}

/// Compute the SHA-256 digest of `data` and return it as a base64 string.
#[allow(dead_code)]
pub fn sha256_base64(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    BASE64.encode(&digest)
}

/// Compute the SHA-256 digest of `data` and return it as a lowercase hex string.
#[allow(dead_code)]
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Create a content-hashed filename like `script.a8f2c3d.js`.
/// `hex_hash` is the full hex SHA-256 digest; we take the first 8 characters.
/// Preserves any directory prefix (e.g. `images/icon.svg` → `images/icon.a8f2c3d.svg`).
#[allow(dead_code)]
pub fn hashed_filename(filename: &str, hex_hash: &str) -> String {
    let short_hash = &hex_hash[..8.min(hex_hash.len())];
    let path = Path::new(filename);
    let stem = path.file_stem().unwrap().to_str().unwrap();
    let ext = path.extension().unwrap().to_str().unwrap();
    let hashed = format!("{stem}.{short_hash}.{ext}");
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            format!("{}/{hashed}", parent.to_str().unwrap())
        }
        _ => hashed,
    }
}

/// Gzip-compress `data` at the highest compression level.
/// Writes the compressed output to `path` and the uncompressed input to `{path}.raw`.
/// Returns the size of the compressed output.
pub fn compress_to_gzip(data: &[u8], path: &str) -> usize {
    // Ensure parent directories exist for nested paths (e.g. images/nested-test.svg.gz)
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).expect("failed to create gzip parent directory");
    }
    let mut encoder = GzEncoder::new(Vec::with_capacity(data.len()), Compression::best());
    encoder.write_all(data).expect("gzip write failed");
    let compressed = encoder.finish().expect("gzip finish failed");
    let compressed_len = compressed.len();
    fs::write(path, &compressed).expect("failed to write gzip file");
    // Also write the uncompressed version for size comparison.
    let raw_path = format!("{path}.raw");
    fs::write(&raw_path, data).expect("failed to write raw file");
    compressed_len
}

/// Escape arbitrary bytes so they can appear inside a Rust `b"..."` literal.
pub fn escape_byte_string(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len());
    for &b in data {
        match b {
            b'\r' => s.push_str("\\r"),
            b'\n' => s.push_str("\\n"),
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7E => s.push(b as char),
            _ => s.push_str(&format!("\\x{b:02X}")),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ── mime_for_file: project's extension → MIME mapping ──────────

    #[test]
    fn mime_text_types_include_charset() {
        assert_eq!(mime_for_file("index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for_file("style.css"), "text/css; charset=utf-8");
        assert_eq!(mime_for_file("script.js"), "text/javascript; charset=utf-8");
        assert_eq!(
            mime_for_file("module.mjs"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_for_file("readme.txt"), "text/plain; charset=utf-8");
        assert_eq!(mime_for_file("data.csv"), "text/csv; charset=utf-8");
    }

    #[test]
    fn mime_image_types() {
        assert_eq!(mime_for_file("icon.png"), "image/png");
        assert_eq!(mime_for_file("photo.jpg"), "image/jpeg");
        assert_eq!(mime_for_file("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_for_file("anim.gif"), "image/gif");
        assert_eq!(mime_for_file("hero.webp"), "image/webp");
        assert_eq!(mime_for_file("favicon.ico"), "image/x-icon");
        assert_eq!(mime_for_file("photo.avif"), "image/avif");
        assert_eq!(mime_for_file("logo.svg"), "image/svg+xml");
    }

    #[test]
    fn mime_font_types() {
        assert_eq!(mime_for_file("font.woff"), "font/woff");
        assert_eq!(mime_for_file("font.woff2"), "font/woff2");
        assert_eq!(mime_for_file("font.ttf"), "font/ttf");
        assert_eq!(mime_for_file("font.otf"), "font/otf");
    }

    #[test]
    fn mime_other_types() {
        assert_eq!(mime_for_file("lib.wasm"), "application/wasm");
        assert_eq!(mime_for_file("doc.pdf"), "application/pdf");
        assert_eq!(mime_for_file("data.json"), "application/json");
        assert_eq!(mime_for_file("feed.xml"), "application/xml");
        assert_eq!(mime_for_file("video.mp4"), "video/mp4");
        assert_eq!(mime_for_file("video.webm"), "video/webm");
        assert_eq!(mime_for_file("song.mp3"), "audio/mpeg");
        assert_eq!(mime_for_file("sound.ogg"), "audio/ogg");
    }

    #[test]
    fn mime_falls_back_to_octet_stream() {
        assert_eq!(mime_for_file("README"), "application/octet-stream");
        assert_eq!(mime_for_file(""), "application/octet-stream");
        assert_eq!(mime_for_file("file.xyz"), "application/octet-stream");
    }

    // ── file_to_const: filename → Rust const identifier ──────────

    #[test]
    fn file_to_const_transforms_correctly() {
        assert_eq!(file_to_const("script.js"), "SCRIPT_JS");
        assert_eq!(file_to_const("style.css"), "STYLE_CSS");
        assert_eq!(file_to_const("index.html"), "INDEX_HTML");
        assert_eq!(file_to_const("my-file.js"), "MY_FILE_JS");
        assert_eq!(file_to_const("jquery.min.js"), "JQUERY_MIN_JS");
    }

    #[test]
    fn file_to_const_prepends_f_for_leading_digit() {
        assert_eq!(file_to_const("404.html"), "F_404_HTML");
    }

    #[test]
    fn file_to_const_handles_nested_paths() {
        assert_eq!(
            file_to_const("images/nested-test.svg"),
            "IMAGES_NESTED_TEST_SVG"
        );
        assert_eq!(file_to_const("css/theme.css"), "CSS_THEME_CSS");
    }

    // ── url_paths_for_file: URL routing aliases ──────────────────

    #[test]
    fn html_files_get_extensionless_alias() {
        assert_eq!(
            url_paths_for_file("about.html"),
            vec!["/about.html", "/about"]
        );
    }

    #[test]
    fn index_html_also_serves_root() {
        assert_eq!(url_paths_for_file("index.html"), vec!["/index.html", "/"]);
    }

    #[test]
    fn non_html_files_have_no_alias() {
        assert_eq!(url_paths_for_file("script.js"), vec!["/script.js"]);
        assert_eq!(url_paths_for_file("style.css"), vec!["/style.css"]);
    }

    // ── header_set_key: deterministic header dedup key ───────────

    #[test]
    fn header_key_sorts_and_is_order_independent() {
        let a = vec![("b".into(), "1".into()), ("a".into(), "2".into())];
        let b = vec![("a".into(), "2".into()), ("b".into(), "1".into())];
        assert_eq!(header_set_key(&a), header_set_key(&b));
        assert_eq!(header_set_key(&a), "a:2\nb:1");
    }

    #[test]
    fn header_key_empty_produces_empty_string() {
        assert_eq!(header_set_key(&[]), "");
    }

    // ── hashed_filename: content-hash injection ──────────────────

    #[test]
    fn hashed_filename_inserts_first_8_chars_of_hash() {
        let hash = "abc123def4567890";
        assert_eq!(hashed_filename("script.js", hash), "script.abc123de.js");
        assert_eq!(hashed_filename("style.css", hash), "style.abc123de.css");
    }

    #[test]
    fn hashed_filename_handles_short_hash() {
        assert_eq!(hashed_filename("file.js", "abc"), "file.abc.js");
        assert_eq!(hashed_filename("file.js", ""), "file..js");
    }

    #[test]
    fn hashed_filename_preserves_directory_prefix() {
        assert_eq!(
            hashed_filename("images/icon.svg", "abc123def4567890"),
            "images/icon.abc123de.svg"
        );
        assert_eq!(
            hashed_filename("css/theme.css", "ff00ff00ff00ff00"),
            "css/theme.ff00ff00.css"
        );
    }

    // ── compress_to_gzip: verify valid gzip + raw copy ───────────

    #[test]
    fn compress_to_gzip_produces_valid_output() {
        let dir = tempdir().unwrap();
        let gz_path = dir.path().join("test.gz").to_str().unwrap().to_string();
        let raw_path = dir.path().join("test.gz.raw");

        let data = b"Hello, world! Test data for gzip.";
        let len = compress_to_gzip(data, &gz_path);

        assert!(len > 0);
        assert!(raw_path.exists());
        assert_eq!(fs::read(&raw_path).unwrap(), data);

        let gz = fs::read(&gz_path).unwrap();
        assert_eq!(&gz[..2], &[0x1F, 0x8B]); // gzip magic bytes
    }

    // ── escape_byte_string: safe byte→string for codegen ─────────

    #[test]
    fn escape_preserves_printable_ascii() {
        assert_eq!(escape_byte_string(b"hello"), "hello");
    }

    #[test]
    fn escape_handles_special_characters() {
        assert_eq!(escape_byte_string(b"a\nb"), "a\\nb");
        assert_eq!(escape_byte_string(b"a\rb"), "a\\rb");
        assert_eq!(escape_byte_string(b"a\tb"), "a\\tb");
        assert_eq!(escape_byte_string(b"a\\b"), "a\\\\b");
        assert_eq!(escape_byte_string(b"a\"b"), "a\\\"b");
    }

    #[test]
    fn escape_handles_non_printable_bytes() {
        assert_eq!(escape_byte_string(&[0x00, 0x01]), "\\x00\\x01");
        assert_eq!(escape_byte_string(&[0xFF]), "\\xFF");
        assert_eq!(escape_byte_string(b""), "");
    }
}
