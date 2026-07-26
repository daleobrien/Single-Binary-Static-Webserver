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
