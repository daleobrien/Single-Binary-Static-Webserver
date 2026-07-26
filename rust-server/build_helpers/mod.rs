mod codegen;
mod processing;
mod tls;
mod utils;

use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use codegen::{AssetGen, CodegenCtx};

pub fn run() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let gzip_dir = format!("{out_dir}/gzip");

    // Clean and recreate the gzip output directory
    let _ = fs::remove_dir_all(&gzip_dir);
    fs::create_dir_all(&gzip_dir).expect("failed to create gzip dir");

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

    // ── CSP and security headers ──
    let (security_headers, csp_base) =
        build_security_headers(&file_hashes, &csp_script_hash);

    // ── Build asset metadata and header deduplication ──
    let (assets, asset_header_indices, header_sets, max_path_len, max_size, has_404, use_uncompressed) =
        build_asset_metadata(&files, &gzip_dir, &security_headers, &csp_base, &file_hashes, &hashed_filenames, &uncompressed_lens, &build_version);

    // ── Version asset ──
    let (version_header_idx, version_len, version_use_uncompressed, header_sets) =
        build_version_headers(
            &build_version,
            &gzip_dir,
            header_sets,
        );

    // ── 404 header set ──
    let (not_found_header_idx, not_found_use_uncompressed, header_sets) =
        build_not_found_headers(has_404, &security_headers, &csp_base, &file_hashes, &gzip_dir, &uncompressed_lens, header_sets, &build_version);

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
    codegen::generate(&ctx);
}

// ── Phase: Source file collection ──────────────────────────────────

fn collect_source_files() -> Vec<String> {
    let public_dir = "../public";
    let mut files: Vec<String> = Vec::new();
    for entry in fs::read_dir(public_dir).expect("failed to read public/") {
        let entry = entry.expect("failed to read entry");
        if entry.file_type().unwrap().is_file() {
            files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files.sort();
    files
}

// ── Phase: Version hash ────────────────────────────────────────────

fn compute_version_hash() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before Unix epoch");
    let mut build_hasher = std::collections::hash_map::DefaultHasher::new();
    now.hash(&mut build_hasher);
    format!("{:016x}", build_hasher.finish())
}

// ── Phase: Version script ──────────────────────────────────────────

fn build_version_script(build_version: &str) -> (String, String) {
    let version_script_src = format!(
        r#"(function(){{const v="{build_version}";async function c(){{try{{const r=await fetch("/v",{{cache:"no-store",headers:{{"If-None-Match":v}}}});if(r.status===304)return;if(r.ok){{fetch(window.location.href,{{cache:"reload"}}).then(()=>location.reload())}}}}catch(_){{}}}}const[n]=performance.getEntriesByType("navigation");if(n&&n.transferSize===0)c();setInterval(c,60000)}})()"#
    );
    let version_js_minified = processing::minify_js_bytes(&version_script_src);
    let version_script_tag = format!(
        "<script>{}</script>",
        String::from_utf8_lossy(&version_js_minified)
    );
    let csp_script_hash = utils::sha256_base64(&version_js_minified);
    (version_script_tag, csp_script_hash)
}

// ── Phase: Two-pass file processing ────────────────────────────────

/// Pass 1: Minify CSS/JS, compute SHA-256 hashes, gzip.
/// Returns (file_hashes: base64 SHA-256, hashed_filenames: name.hash.ext).
fn minify_compute_sha_and_compress(
    files: &[String],
    gzip_dir: &str,
    uncompressed_lens: &mut HashMap<String, usize>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut file_hashes: HashMap<String, String> = HashMap::new();
    let mut hashed_filenames: HashMap<String, String> = HashMap::new();

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
        let minified = processing::minify_file(file, &raw);

        // Compute SHA-256 of the (uncompressed) body for the Digest header.
        file_hashes.insert(file.clone(), utils::sha256_base64(&minified));

        // Compute hex SHA-256 for content-hashed filename (e.g. script.a8f2c3d.js).
        let hex_hash = utils::sha256_hex(&minified);
        hashed_filenames.insert(file.clone(), utils::hashed_filename(file, &hex_hash));

        // Track uncompressed length for later size comparison.
        uncompressed_lens.insert(file.clone(), minified.len());

        utils::compress_to_gzip(&minified, &format!("{gzip_dir}/{file}.gz"));
    }

    (file_hashes, hashed_filenames)
}

/// Pass 2: Process HTML — inject version script + SRI integrity attributes,
/// replace original JS/CSS filenames with content-hashed versions, then minify + gzip.
fn update_html_sri_and_inject_update_js(
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

// ── Phase: CSP + security headers ─────────────────────────────────

fn build_security_headers(
    file_hashes: &HashMap<String, String>,
    csp_script_hash: &str,
) -> (Vec<(String, String)>, String) {
    let mut csp_css_hashes: Vec<String> = Vec::new();
    let mut csp_js_hashes: Vec<String> = Vec::new();
    for (file, hash) in file_hashes {
        match Path::new(file).extension().and_then(|e| e.to_str()) {
            Some("css") => csp_css_hashes.push(hash.clone()),
            Some("js") => csp_js_hashes.push(hash.clone()),
            _ => {}
        }
    }

    let sha = |h: &String| format!("'sha256-{h}'");

    let csp_css_part: String = {
        let mut parts: Vec<String> = vec!["'self'".into()];
        parts.extend(csp_css_hashes.iter().map(&sha));
        parts.join(" ")
    };

    let csp_js_part: String = {
        let mut parts: Vec<String> = vec![sha(&csp_script_hash.to_string())];
        parts.extend(csp_js_hashes.iter().map(&sha));
        parts.join(" ")
    };

    let non_csp_headers: Vec<(String, String)> = vec![
        ("x-content-type-options".into(), "nosniff".into()),
        ("x-frame-options".into(), "DENY".into()),
        ("x-xss-protection".into(), "1; mode=block".into()),
        (
            "referrer-policy".into(),
            "strict-origin-when-cross-origin".into(),
        ),
        (
            "strict-transport-security".into(),
            "max-age=31536000; includeSubDomains".into(),
        ),
        (
            "permissions-policy".into(),
            "camera=(), microphone=(), geolocation=()".into(),
        ),
        ("alt-svc".into(), "h3=\\\":3000\\\"".into()),
    ];

    let csp_base = format!(
        "default-src 'none'; style-src {csp_css_part}; script-src {csp_js_part}; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; connect-src 'self'"
    );

    (non_csp_headers, csp_base)
}

/// Check whether a source HTML file references any images (via `<img>`, `<link rel="icon">`,
/// or `<link rel="shortcut icon">`). Reads from `../public/` at build time.
fn page_has_images(file: &str) -> bool {
    let source = fs::read_to_string(format!("../public/{file}"))
        .unwrap_or_default();
    let lower = source.to_lowercase();
    lower.contains("<img")
        || (lower.contains("rel=\"icon\"") || lower.contains("rel='icon'"))
        || (lower.contains("rel=\"shortcut icon\"") || lower.contains("rel='shortcut icon'"))
}

// ── Phase: Asset metadata ──────────────────────────────────────────

fn build_asset_metadata(
    files: &[String],
    gzip_dir: &str,
    security_headers: &[(String, String)],
    csp_base: &str,
    file_hashes: &HashMap<String, String>,
    hashed_filenames: &HashMap<String, String>,
    uncompressed_lens: &HashMap<String, usize>,
    build_version: &str,
) -> (
    Vec<AssetGen>,
    Vec<usize>,
    Vec<Vec<(String, String)>>,
    usize,
    usize,
    bool,
    Vec<bool>,
) {
    let mut header_sets: Vec<Vec<(String, String)>> = Vec::new();
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    let mut assets: Vec<AssetGen> = Vec::new();
    let mut asset_header_indices: Vec<usize> = Vec::new();
    let mut use_uncompressed: Vec<bool> = Vec::new();
    let mut has_404 = false;
    let mut max_path_len: usize = 0;
    let mut max_size: usize = 0;

    for file in files {
        let content_type = utils::mime_for_file(file);
        let const_prefix = utils::file_to_const(file);
        // Use the content-hashed filename for URL paths when available.
        let url_file = hashed_filenames
            .get(file)
            .map(|s| s.as_str())
            .unwrap_or(file);
        let url_paths = utils::url_paths_for_file(url_file);

        for path in &url_paths {
            max_path_len = max_path_len.max(path.len());
        }

        if file == "404.html" {
            has_404 = true;
        }

        let gz_name = format!("{file}.gz");
        let gz_path = format!("{gzip_dir}/{gz_name}");
        let gz_data = fs::read(&gz_path).expect("failed to read gzipped file");
        let uncompressed_len = uncompressed_lens.get(file).copied().unwrap_or(gz_data.len());
        let use_uncomp = uncompressed_len < gz_data.len();
        use_uncompressed.push(use_uncomp);

        let (body_data, content_length) = if use_uncomp {
            let raw_path = format!("{gz_path}.raw");
            let raw_data = fs::read(&raw_path).expect("failed to read raw file");
            let len = raw_data.len();
            (raw_data, len)
        } else {
            let len = gz_data.len();
            (gz_data, len)
        };
        max_size = max_size.max(content_length);

        // Per-page CSP: only allow images on pages that actually reference them.
        let img_src = if content_type.starts_with("text/html") && page_has_images(file) {
            "img-src 'self'"
        } else {
            "img-src 'none'"
        };
        let csp_value = format!("{img_src}; {csp_base}");

        // Build header set for this asset
        let mut headers: Vec<(String, String)> = Vec::new();
        headers.push(("content-type".into(), content_type.into()));
        if !use_uncomp {
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
    )
}

// ── Phase: Version endpoint headers ────────────────────────────────

fn build_version_headers(
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

// ── Phase: 404 headers ─────────────────────────────────────────────

fn build_not_found_headers(
    has_404: bool,
    security_headers: &[(String, String)],
    csp_base: &str,
    file_hashes: &HashMap<String, String>,
    gzip_dir: &str,
    uncompressed_lens: &HashMap<String, usize>,
    mut header_sets: Vec<Vec<(String, String)>>,
    build_version: &str,
) -> (usize, bool, Vec<Vec<(String, String)>>) {
    let mut not_found_headers: Vec<(String, String)> = Vec::new();
    not_found_headers.push(("content-type".into(), "text/html; charset=utf-8".into()));
    let mut not_found_use_uncomp = false;
    let gz_404 = if has_404 {
        let gz = fs::read(format!("{gzip_dir}/404.html.gz"))
            .expect("failed to read 404 gzip");
        let orig_len = uncompressed_lens.get("404.html").copied().unwrap_or(0);
        not_found_use_uncomp = orig_len < gz.len();
        if !not_found_use_uncomp {
            not_found_headers.push(("content-encoding".into(), "gzip".into()));
        }
        Some(gz)
    } else {
        None
    };
    // 404 pages never reference images — lock down img-src.
    let csp_value = format!("img-src 'none'; {csp_base}");
    not_found_headers.push(("content-security-policy".into(), csp_value));
    not_found_headers.extend_from_slice(security_headers);
    not_found_headers.push(("cache-control".into(), "public, max-age=3600".into()));
    not_found_headers.push(("etag".into(), build_version.to_string()));

    // Rebuild index from existing header_sets
    let mut header_set_index: HashMap<String, usize> = HashMap::new();
    for (i, set) in header_sets.iter().enumerate() {
        header_set_index.insert(utils::header_set_key(set), i);
    }

    let not_found_header_idx = if has_404 {
        // Repr-Digest from the uncompressed 404 HTML
        if let Some(hash) = file_hashes.get("404.html") {
            not_found_headers.push((
                "repr-digest".into(),
                format!("sha-256={}", hash),
            ));
        }
        // Content-Digest from the body actually sent
        let content_digest_data = if not_found_use_uncomp {
            let raw_path = format!("{gzip_dir}/404.html.gz.raw");
            fs::read(&raw_path).expect("failed to read 404 raw")
        } else {
            gz_404.expect("gz_404 must be Some when has_404 and not using uncompressed")
        };
        not_found_headers.push((
            "content-digest".into(),
            format!("sha-256={}", utils::sha256_base64(&content_digest_data)),
        ));

        let key = utils::header_set_key(&not_found_headers);
        *header_set_index.entry(key).or_insert_with(|| {
            let i = header_sets.len();
            header_sets.push(not_found_headers);
            i
        })
    } else {
        let body: &[u8] = b"<h1>404 - Not Found</h1>";
        let hash = utils::sha256_base64(body);
        let cl = body.len().to_string();
        let mut h = not_found_headers;
        h.push(("content-length".into(), cl.clone()));
        h.push(("repr-digest".into(), format!("sha-256={}", hash)));
        h.push(("content-digest".into(), format!("sha-256={}", hash)));
        let key = utils::header_set_key(&h);
        *header_set_index.entry(key).or_insert_with(|| {
            let i = header_sets.len();
            header_sets.push(h);
            i
        })
    };

    (not_found_header_idx, not_found_use_uncomp, header_sets)
}
