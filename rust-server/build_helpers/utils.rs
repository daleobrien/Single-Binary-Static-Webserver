use std::fs;
use std::io::Write;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};

/// Map a filename's extension to its MIME type.
pub fn mime_for_file(filename: &str) -> &'static str {
    match Path::new(filename).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        _ => "application/octet-stream",
    }
}

/// Convert a filename into a valid Rust const identifier prefix.
///
/// Example: `"script.js"` → `"SCRIPT_JS"`, `"404.html"` → `"F_404_HTML"`
pub fn file_to_const(filename: &str) -> String {
    let s = filename.replace('.', "_").replace('-', "_").to_uppercase();
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
pub fn sha256_base64(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    BASE64.encode(&digest)
}

/// Compute the SHA-256 digest of `data` and return it as a lowercase hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("{:x}", digest)
}

/// Create a content-hashed filename like `script.a8f2c3d.js`.
/// `hex_hash` is the full hex SHA-256 digest; we take the first 8 characters.
pub fn hashed_filename(filename: &str, hex_hash: &str) -> String {
    let short_hash = &hex_hash[..8.min(hex_hash.len())];
    let path = Path::new(filename);
    let stem = path.file_stem().unwrap().to_str().unwrap();
    let ext = path.extension().unwrap().to_str().unwrap();
    format!("{stem}.{short_hash}.{ext}")
}

/// Gzip-compress `data` at the highest compression level.
/// Writes the compressed output to `path` and the uncompressed input to `{path}.raw`.
/// Returns the size of the compressed output.
pub fn compress_to_gzip(data: &[u8], path: &str) -> usize {
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
