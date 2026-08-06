use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Precomputed CSP values for directive types whose sources can be hashed.
/// Each field holds the full value string (e.g. `"'self' 'sha256-abc'"`) that
/// should appear in the directive when the page uses that resource type.
pub(super) struct CspValues {
    pub script_src: String,
    pub style_src: String,
    pub img_src: String,
    pub font_src: String,
    pub media_src: String,
    pub frame_src: String,
}

/// Build `CspValues` by filtering `file_hashes` by extension.
/// Called once per build, not once per HTML page.
pub(super) fn build_csp_values(
    file_hashes: &HashMap<String, String>,
    csp_script_hash: &str,
    disable_sri: bool,
) -> CspValues {
    if disable_sri {
        // When SRI is disabled, omit all sha256 hashes from CSP.
        // The version-check script hash is also omitted since it's injected inline.
        return CspValues {
            script_src: "'self' 'unsafe-inline'".to_string(),
            style_src: "'self' 'unsafe-inline'".to_string(),
            img_src: "'self'".to_string(),
            font_src: "'self'".to_string(),
            media_src: "'self'".to_string(),
            frame_src: "'self'".to_string(),
        };
    }

    let js_hashes = collect_hashes(file_hashes, &[".js"]);
    let css_hashes = collect_hashes(file_hashes, &[".css"]);
    let img_hashes = collect_hashes(
        file_hashes,
        &[".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".ico"],
    );
    let font_hashes = collect_hashes(file_hashes, &[".woff", ".woff2", ".ttf", ".otf"]);
    let media_hashes = collect_hashes(file_hashes, &[".mp3", ".mp4", ".webm", ".ogg", ".wav"]);

    CspValues {
        script_src: {
            let mut parts = vec![format!("'sha256-{csp_script_hash}'")];
            parts.extend(js_hashes);
            parts.join(" ")
        },
        style_src: join_value("'self' 'unsafe-inline'", &css_hashes),
        img_src: join_value("'self'", &img_hashes),
        font_src: join_value("'self'", &font_hashes),
        media_src: join_value("'self'", &media_hashes),
        frame_src: "'self'".to_string(),
    }
}

/// Build a fully data-driven CSP by analysing the source HTML for referenced
/// resource types. Non-HTML assets get a minimal `default-src 'none'` —
/// only the HTML page's CSP governs what the browser loads.
#[allow(dead_code)] // only called from the build script, not from tests
pub(super) fn build_csp(file: &str, values: &CspValues) -> String {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext != "html" {
        return "default-src 'none'".to_string();
    }

    let source = fs::read_to_string(format!("../public/{file}")).unwrap_or_default();
    build_csp_for_source(&source, values)
}

/// Pure core of `build_csp`: takes the HTML source string directly so it can
/// be unit-tested without touching the filesystem.
fn build_csp_for_source(source: &str, values: &CspValues) -> String {
    let lower = source.to_lowercase();

    // Quick substring check helper
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));

    let mut directives: Vec<String> = vec!["default-src 'none'".into()];

    // Every directive type is handled uniformly:
    //   present → "{name} {precomputed_value}"
    //   absent  → "{name} 'none'"
    for (name, value, present) in [
        ("script-src", &values.script_src, true), // version-check script always injected
        (
            "style-src",
            &values.style_src,
            has(&["stylesheet", "<style"]),
        ),
        (
            "img-src",
            &values.img_src,
            has(&["<img", "rel=\"icon\"", "rel='icon'"]),
        ),
        ("font-src", &values.font_src, has(&["font-", "@font-face"])),
        ("media-src", &values.media_src, has(&["<audio", "<video"])),
        ("frame-src", &values.frame_src, has(&["<iframe"])),
    ] {
        directives.push(if present {
            format!("{name} {value}")
        } else {
            format!("{name} 'none'")
        });
    }

    // Always-present directives
    directives.push("connect-src 'self'".into());
    directives.push("object-src 'none'".into());
    directives.push("base-uri 'self'".into());
    directives.push("form-action 'self'".into());
    directives.push("frame-ancestors 'none'".into());

    directives.join("; ")
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Collect sha256 hashes for files matching any of the given extensions.
fn collect_hashes(file_hashes: &HashMap<String, String>, exts: &[&str]) -> Vec<String> {
    file_hashes
        .iter()
        .filter(|(f, _)| exts.iter().any(|e| f.ends_with(e)))
        .map(|(_, h)| format!("'sha256-{h}'"))
        .collect()
}

/// Join a base value with file hashes into a single space-separated string.
fn join_value(base: &str, hashes: &[String]) -> String {
    let mut parts = vec![base.to_string()];
    parts.extend(hashes.iter().cloned());
    parts.join(" ")
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Helper: build a CspValues with simple, predictable contents.
    fn test_values() -> CspValues {
        CspValues {
            script_src: "'sha256-script'".into(),
            style_src: "'self' 'sha256-css'".into(),
            img_src: "'self' 'sha256-img'".into(),
            font_src: "'self' 'sha256-font'".into(),
            media_src: "'self' 'sha256-media'".into(),
            frame_src: "'self'".into(),
        }
    }

    // Helper: extract a single directive value from a CSP string.
    fn directive<'a>(csp: &'a str, name: &str) -> &'a str {
        let prefix = format!("{name} ");
        csp.split("; ")
            .find(|d| d.starts_with(&prefix))
            .map(|d| &d[prefix.len()..])
            .unwrap_or("")
    }

    // ── collect_hashes ──────────────────────────────────────────────

    #[test]
    fn collect_hashes_empty_map() {
        let map: HashMap<String, String> = HashMap::new();
        assert!(collect_hashes(&map, &[".js"]).is_empty());
    }

    #[test]
    fn collect_hashes_no_matches() {
        let map = HashMap::from([
            ("a.css".into(), "aaa".into()),
            ("b.css".into(), "bbb".into()),
        ]);
        assert!(collect_hashes(&map, &[".js"]).is_empty());
    }

    #[test]
    fn collect_hashes_single_match() {
        let map = HashMap::from([("app.js".into(), "abc".into())]);
        assert_eq!(collect_hashes(&map, &[".js"]), vec!["'sha256-abc'"]);
    }

    #[test]
    fn collect_hashes_multiple_matches() {
        let map = HashMap::from([("a.js".into(), "111".into()), ("b.js".into(), "222".into())]);
        let result = collect_hashes(&map, &[".js"]);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"'sha256-111'".to_string()));
        assert!(result.contains(&"'sha256-222'".to_string()));
    }

    #[test]
    fn collect_hashes_mixed_extensions() {
        let map = HashMap::from([
            ("a.js".into(), "js".into()),
            ("b.css".into(), "css".into()),
            ("c.png".into(), "png".into()),
            ("d.woff2".into(), "font".into()),
        ]);
        // Only .js and .css matched
        let result = collect_hashes(&map, &[".js", ".css"]);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"'sha256-js'".to_string()));
        assert!(result.contains(&"'sha256-css'".to_string()));
    }

    // ── join_value ──────────────────────────────────────────────────

    #[test]
    fn join_value_just_base() {
        let empty: Vec<String> = vec![];
        assert_eq!(join_value("'self'", &empty), "'self'");
    }

    #[test]
    fn join_value_with_hashes() {
        let hashes = vec!["'sha256-aaa'".to_string(), "'sha256-bbb'".to_string()];
        assert_eq!(
            join_value("'self'", &hashes),
            "'self' 'sha256-aaa' 'sha256-bbb'"
        );
    }

    // ── build_csp_values ────────────────────────────────────────────

    #[test]
    fn csp_values_no_hashes() {
        let map = HashMap::new();
        let v = build_csp_values(&map, "scripthash", false);
        assert_eq!(v.script_src, "'sha256-scripthash'");
        assert_eq!(v.style_src, "'self' 'unsafe-inline'");
        assert_eq!(v.img_src, "'self'");
        assert_eq!(v.font_src, "'self'");
        assert_eq!(v.media_src, "'self'");
        assert_eq!(v.frame_src, "'self'");
    }

    #[test]
    fn csp_values_with_js_and_css() {
        let map = HashMap::from([
            ("app.js".into(), "js123".into()),
            ("lib.js".into(), "js456".into()),
            ("main.css".into(), "css789".into()),
        ]);
        let v = build_csp_values(&map, "scripthash", false);
        assert!(v.script_src.contains("'sha256-scripthash'"));
        assert!(v.script_src.contains("'sha256-js123'"));
        assert!(v.script_src.contains("'sha256-js456'"));
        assert_eq!(v.style_src, "'self' 'unsafe-inline' 'sha256-css789'");
        // Non-CSS/JS should stay as bare 'self'
        assert_eq!(v.img_src, "'self'");
        assert_eq!(v.font_src, "'self'");
        assert_eq!(v.media_src, "'self'");
    }

    #[test]
    fn csp_values_with_images_and_fonts() {
        let map = HashMap::from([
            ("logo.png".into(), "img1".into()),
            ("icon.svg".into(), "img2".into()),
            ("roboto.woff2".into(), "fnt1".into()),
            ("fallback.ttf".into(), "fnt2".into()),
        ]);
        let v = build_csp_values(&map, "scripthash", false);
        assert_eq!(v.script_src, "'sha256-scripthash'");
        assert_eq!(v.style_src, "'self' 'unsafe-inline'");
        assert!(v.img_src.contains("'sha256-img1'"));
        assert!(v.img_src.contains("'sha256-img2'"));
        assert!(v.font_src.contains("'sha256-fnt1'"));
        assert!(v.font_src.contains("'sha256-fnt2'"));
        assert_eq!(v.media_src, "'self'");
    }

    #[test]
    fn csp_values_disabled_sri() {
        let map = HashMap::from([
            ("app.js".into(), "js123".into()),
            ("main.css".into(), "css789".into()),
            ("logo.png".into(), "img1".into()),
        ]);
        let v = build_csp_values(&map, "scripthash", true);
        // When SRI is disabled, CSP uses 'self' and 'unsafe-inline' for scripts/styles
        // to allow the inline version-check script and any inline styles.
        assert_eq!(v.script_src, "'self' 'unsafe-inline'");
        assert_eq!(v.style_src, "'self' 'unsafe-inline'");
        assert_eq!(v.img_src, "'self'");
        assert_eq!(v.font_src, "'self'");
        assert_eq!(v.media_src, "'self'");
        assert_eq!(v.frame_src, "'self'");
    }

    // ── build_csp_for_source ────────────────────────────────────────

    #[test]
    fn csp_empty_page() {
        let html = "<!DOCTYPE html><html><head></head><body></body></html>";
        let csp = build_csp_for_source(html, &test_values());

        // Only script-src should be active (version-check script is always present)
        assert_eq!(directive(&csp, "default-src"), "'none'");
        assert_eq!(directive(&csp, "script-src"), "'sha256-script'");
        assert_eq!(directive(&csp, "style-src"), "'none'");
        assert_eq!(directive(&csp, "img-src"), "'none'");
        assert_eq!(directive(&csp, "font-src"), "'none'");
        assert_eq!(directive(&csp, "media-src"), "'none'");
        assert_eq!(directive(&csp, "frame-src"), "'none'");
        // Always-present
        assert_eq!(directive(&csp, "connect-src"), "'self'");
        assert_eq!(directive(&csp, "object-src"), "'none'");
        assert_eq!(directive(&csp, "base-uri"), "'self'");
        assert_eq!(directive(&csp, "form-action"), "'self'");
        assert_eq!(directive(&csp, "frame-ancestors"), "'none'");
    }

    #[test]
    fn csp_with_stylesheet_link() {
        let html = r#"<head><link rel="stylesheet" href="/main.css"></head>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "style-src"), "'self' 'sha256-css'");
        assert_eq!(directive(&csp, "img-src"), "'none'");
    }

    #[test]
    fn csp_with_inline_style() {
        let html = "<head><style>body { color: red; }</style></head>";
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "style-src"), "'self' 'sha256-css'");
    }

    #[test]
    fn csp_with_img_tag() {
        let html = r#"<body><img src="/logo.png" alt="logo"></body>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "img-src"), "'self' 'sha256-img'");
        assert_eq!(directive(&csp, "style-src"), "'none'");
    }

    #[test]
    fn csp_with_favicon_double_quotes() {
        let html = r#"<head><link rel="icon" href="/favicon.ico"></head>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "img-src"), "'self' 'sha256-img'");
    }

    #[test]
    fn csp_with_favicon_single_quotes() {
        let html = "<head><link rel='icon' href='/favicon.ico'></head>";
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "img-src"), "'self' 'sha256-img'");
    }

    #[test]
    fn csp_with_font_face() {
        let html = "<style>@font-face { font-family: 'Roboto'; }</style>";
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "font-src"), "'self' 'sha256-font'");
    }

    #[test]
    fn csp_with_font_property() {
        // "font-" prefix in CSS property names like font-family, font-size, etc.
        let html = r#"<body style="font-family: sans-serif">"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "font-src"), "'self' 'sha256-font'");
    }

    #[test]
    fn csp_with_audio() {
        let html = r#"<audio src="/sound.mp3" controls></audio>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "media-src"), "'self' 'sha256-media'");
    }

    #[test]
    fn csp_with_video() {
        let html = r#"<video src="/clip.mp4" controls></video>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "media-src"), "'self' 'sha256-media'");
    }

    #[test]
    fn csp_with_iframe() {
        let html = r#"<iframe src="https://example.com"></iframe>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "frame-src"), "'self'");
    }

    #[test]
    fn csp_with_all_resource_types() {
        let html = r#"
            <!DOCTYPE html><html><head>
            <link rel="stylesheet" href="/main.css">
            <link rel="icon" href="/favicon.ico">
            <style>@font-face { font-family: 'X'; }</style>
            </head><body>
            <img src="/logo.png">
            <audio src="/sound.mp3"></audio>
            <video src="/clip.mp4"></video>
            <iframe src="https://example.com"></iframe>
            </body></html>
        "#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "script-src"), "'sha256-script'");
        assert_eq!(directive(&csp, "style-src"), "'self' 'sha256-css'");
        assert_eq!(directive(&csp, "img-src"), "'self' 'sha256-img'");
        assert_eq!(directive(&csp, "font-src"), "'self' 'sha256-font'");
        assert_eq!(directive(&csp, "media-src"), "'self' 'sha256-media'");
        assert_eq!(directive(&csp, "frame-src"), "'self'");
    }

    #[test]
    fn csp_case_insensitive_detection() {
        // The HTML is lowercased before checking, so uppercase should still match.
        let html = r#"<IMG SRC="/logo.PNG"><STYLE>BODY {}</STYLE><AUDIO>"#;
        let csp = build_csp_for_source(html, &test_values());
        assert_eq!(directive(&csp, "img-src"), "'self' 'sha256-img'");
        assert_eq!(directive(&csp, "style-src"), "'self' 'sha256-css'");
        assert_eq!(directive(&csp, "media-src"), "'self' 'sha256-media'");
    }
}
