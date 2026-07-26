use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use css_minify::optimizations::{Level, Minifier};
use minify_html::{minify as minify_html, Cfg};
use minify_js::{minify as minify_js, Session, TopLevelMode};

/// HTML minification config, with `minify_js` toggled.
pub fn html_cfg(minify_js: bool) -> Cfg {
    Cfg {
        do_not_minify_doctype: false,
        ensure_spec_compliant_unquoted_attribute_values: true,
        keep_closing_tags: false,
        keep_html_and_head_opening_tags: false,
        keep_spaces_between_attributes: true,
        keep_comments: false,
        keep_input_type_text_attr: false,
        keep_ssi_comments: false,
        preserve_brace_template_syntax: false,
        preserve_chevron_percent_template_syntax: false,
        minify_css: true,
        minify_js,
        remove_bangs: false,
        remove_processing_instructions: false,
    }
}

/// Minify a file's content based on its extension.
pub fn minify_file(filename: &str, raw: &[u8]) -> Vec<u8> {
    match Path::new(filename).extension().and_then(|e| e.to_str()) {
        Some("html") => minify_html(raw, &html_cfg(true)),
        Some("css") => Minifier::default()
            .minify(&String::from_utf8_lossy(raw), Level::Three)
            .expect("CSS minification failed")
            .into_bytes(),
        Some("js") => {
            let raw_owned = raw.to_vec();
            let raw_fallback = raw_owned.clone();
            panic::catch_unwind(AssertUnwindSafe(move || {
                let session = Session::new();
                let mut out = Vec::new();
                minify_js(&session, TopLevelMode::Global, &raw_owned, &mut out)
                    .expect("JS minification failed");
                out
            }))
            .unwrap_or_else(|_| {
                eprintln!(
                    "warning: JS minification panicked, falling back to raw content for '{filename}'"
                );
                raw_fallback
            })
        }
        _ => {
            // Unknown types: pass through unmodified
            raw.to_vec()
        }
    }
}

/// Minify a JavaScript string and return the minified bytes.
pub fn minify_js_bytes(js: &str) -> Vec<u8> {
    let session = Session::new();
    let mut out = Vec::new();
    minify_js(&session, TopLevelMode::Global, js.as_bytes(), &mut out)
        .expect("JS minification failed for version script");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── html_cfg: project's minification config ──────────────────

    #[test]
    fn html_cfg_toggles_js_minification_only() {
        let with_js = html_cfg(true);
        assert!(with_js.minify_js);
        assert!(with_js.minify_css);

        let without_js = html_cfg(false);
        assert!(!without_js.minify_js);
        assert!(without_js.minify_css); // CSS is always minified
    }

    // ── minify_file: extension-based routing to minifiers ────────

    #[test]
    fn minify_file_routes_html_to_html_minifier() {
        let input = b"<!DOCTYPE html><html>  <body>  <p>Hi</p>  </body></html>";
        let result = minify_file("page.html", input);
        let s = String::from_utf8_lossy(&result);
        assert!(s.contains("Hi"));
        assert!(result.len() < input.len(), "HTML should be smaller after minification");
    }

    #[test]
    fn minify_file_routes_css_to_css_minifier() {
        let input = b"body {\n    color: red;\n}";
        let result = minify_file("style.css", input);
        let s = String::from_utf8_lossy(&result);
        assert!(s.contains("color"));
        assert!(!s.contains('\n'), "CSS should have no newlines after minification");
    }

    #[test]
    fn minify_file_routes_js_to_js_minifier() {
        let input = b"function add(a, b) {\n    return a + b;\n}";
        let result = minify_file("script.js", input);
        let s = String::from_utf8_lossy(&result);
        assert!(s.contains("return") || s.contains("add"));
        assert!(result.len() <= input.len());
    }

    #[test]
    fn minify_file_passes_through_unknown_types() {
        let input = b"plain text content";
        assert_eq!(minify_file("readme.txt", input), input);
        assert_eq!(minify_file("README", input), input);
    }

    // ── minify_js_bytes: helper used for the version-check script ─

    #[test]
    fn minify_js_bytes_produces_smaller_output() {
        let input = "function  hello(  ) {\n    return  42;\n}";
        let result = minify_js_bytes(input);
        assert!(result.len() < input.len());
    }
}
