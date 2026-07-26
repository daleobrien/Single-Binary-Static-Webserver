// Shim to include build_helpers source modules so `cargo test` runs their #[cfg(test)] blocks.
// The build_helpers are normally compiled only as part of build.rs and uses [build-dependencies].
// We mirror the necessary crates in [dev-dependencies] so this integration test can compile.

#[path = "../build_helpers/utils.rs"]
mod utils;

#[path = "../build_helpers/processing.rs"]
mod processing;

#[path = "../build_helpers/codegen.rs"]
mod codegen;

#[path = "../build_helpers/csp.rs"]
mod csp;

// ── codegen module tests ──────────────────────────────────────────

#[cfg(test)]
mod codegen_tests {
    use super::codegen::{self, AssetGen, CodegenCtx};
    use std::fs;
    use tempfile::tempdir;

    /// Helper to build a minimal CodegenCtx for testing.
    fn minimal_ctx(out_dir: &str, gzip_dir: &str) -> CodegenCtx {
        CodegenCtx {
            out_dir: out_dir.to_string(),
            gzip_dir: gzip_dir.to_string(),
            build_version: "test-version-hash".to_string(),
            assets: vec![AssetGen {
                const_prefix: "INDEX_HTML".to_string(),
                url_paths: vec!["/".to_string(), "/index.html".to_string()],
            }],
            asset_header_indices: vec![0],
            header_sets: vec![vec![
                ("content-type".to_string(), "text/html".to_string()),
            ]],
            version_header_idx: 1,
            version_len: 9,
            not_found_header_idx: 2,
            files: vec!["index.html".to_string()],
            has_404: false,
            max_path_len: 11,
            max_size: 1024,
            use_uncompressed: vec![false],
            version_use_uncompressed: true,
            not_found_use_uncompressed: false,
        }
    }

    // ── Asset struct includes content_length_str ──────────────────

    #[test]
    fn asset_struct_includes_content_length_str_field() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let gzip_dir = dir.path().join("gzip");
        fs::create_dir_all(&gzip_dir).unwrap();

        // Create a minimal gzip body file for index.html
        fs::write(gzip_dir.join("index.html.gz"), b"dummy-gzip-body").unwrap();

        let ctx = minimal_ctx(out_dir, gzip_dir.to_str().unwrap());
        codegen::generate(&ctx);

        let generated = fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // Asset struct should declare content_length_str
        assert!(
            generated.contains("pub content_length_str: &'static str,"),
            "Asset struct must include content_length_str field.\nGenerated:\n{generated}"
        );
    }

    // ── _LEN_STR constants are emitted for each asset ─────────────

    #[test]
    fn asset_constants_include_len_str() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let gzip_dir = dir.path().join("gzip");
        fs::create_dir_all(&gzip_dir).unwrap();

        // Create a gzip body file with known size
        let body = b"hello-world-body";
        fs::write(gzip_dir.join("index.html.gz"), body).unwrap();

        let ctx = minimal_ctx(out_dir, gzip_dir.to_str().unwrap());
        codegen::generate(&ctx);

        let generated = fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        assert!(
            generated.contains("const INDEX_HTML_LEN: usize = 16;"),
            "INDEX_HTML_LEN must match the body size.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const INDEX_HTML_LEN_STR: &str = \"16\";"),
            "INDEX_HTML_LEN_STR must be the string form of the length.\nGenerated:\n{generated}"
        );
    }

    // ── VERSION_LEN_STR is emitted ─────────────────────────────────

    #[test]
    fn version_asset_includes_len_str() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let gzip_dir = dir.path().join("gzip");
        fs::create_dir_all(&gzip_dir).unwrap();

        // write_asset_constants stats every file in ctx.files, so provide a dummy
        fs::write(gzip_dir.join("index.html.gz"), b"x").unwrap();

        let ctx = minimal_ctx(out_dir, gzip_dir.to_str().unwrap());
        codegen::generate(&ctx);

        let generated = fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        assert!(
            generated.contains("const VERSION_LEN: usize = 9;"),
            "VERSION_LEN must match version_len from ctx.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const VERSION_LEN_STR: &str = \"9\";"),
            "VERSION_LEN_STR must be the string form.\nGenerated:\n{generated}"
        );
    }

    // ── NOT_FOUND_LEN_STR is emitted (inline fallback path) ────────

    #[test]
    fn not_found_asset_includes_len_str_inline_fallback() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let gzip_dir = dir.path().join("gzip");
        fs::create_dir_all(&gzip_dir).unwrap();

        // write_asset_constants stats every file in ctx.files, so provide a dummy
        fs::write(gzip_dir.join("index.html.gz"), b"x").unwrap();

        // has_404 = false → inline fallback body: b"<h1>404 - Not Found</h1>" (24 bytes)
        let ctx = minimal_ctx(out_dir, gzip_dir.to_str().unwrap());
        codegen::generate(&ctx);

        let generated = fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        assert!(
            generated.contains("const NOT_FOUND_LEN: usize = 24;"),
            "NOT_FOUND_LEN must be 24 (length of inline 404 body).\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const NOT_FOUND_LEN_STR: &str = \"24\";"),
            "NOT_FOUND_LEN_STR must be the string form.\nGenerated:\n{generated}"
        );
    }

    // ── Asset instances initialize content_length_str ──────────────

    #[test]
    fn asset_instances_initialize_content_length_str() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let gzip_dir = dir.path().join("gzip");
        fs::create_dir_all(&gzip_dir).unwrap();

        fs::write(gzip_dir.join("index.html.gz"), b"dummy-gzip-body").unwrap();

        let ctx = minimal_ctx(out_dir, gzip_dir.to_str().unwrap());
        codegen::generate(&ctx);

        let generated = fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // File asset instance
        assert!(
            generated.contains("content_length_str: INDEX_HTML_LEN_STR,"),
            "Asset instance must initialize content_length_str from INDEX_HTML_LEN_STR.\nGenerated:\n{generated}"
        );
        // Version asset instance
        assert!(
            generated.contains("content_length_str: VERSION_LEN_STR,"),
            "VERSION_ASSET must initialize content_length_str from VERSION_LEN_STR.\nGenerated:\n{generated}"
        );
        // 404 asset instance
        assert!(
            generated.contains("content_length_str: NOT_FOUND_LEN_STR,"),
            "NOT_FOUND_ASSET must initialize content_length_str from NOT_FOUND_LEN_STR.\nGenerated:\n{generated}"
        );
    }

    // ── End-to-end: all constants and struct are consistent ────────

    #[test]
    fn generated_output_is_self_consistent() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let gzip_dir = dir.path().join("gzip");
        fs::create_dir_all(&gzip_dir).unwrap();

        fs::write(gzip_dir.join("index.html.gz"), b"a").unwrap(); // 1 byte

        let mut ctx = minimal_ctx(out_dir, gzip_dir.to_str().unwrap());
        ctx.max_size = 1;

        codegen::generate(&ctx);

        let generated = fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // The LEN_STR must contain the same value as LEN (as a string)
        assert!(generated.contains("const INDEX_HTML_LEN: usize = 1;"));
        assert!(generated.contains("const INDEX_HTML_LEN_STR: &str = \"1\";"));

        // MAX_SIZE_DIGITS should be appropriate (1 digit for max_size=1)
        assert!(generated.contains("pub const MAX_SIZE_DIGITS: usize = 1;"));

        // The struct definition, instance initialization, and the _LEN_STR constant
        // must all reference the same identifier pattern
        let len_str_count = generated.matches("_LEN_STR").count();
        // Expected: 1 declaration + 1 instance usage = 2, plus VERSION and NOT_FOUND:
        // INDEX_HTML_LEN_STR declaration + usage = 2
        // VERSION_LEN_STR declaration + usage = 2
        // NOT_FOUND_LEN_STR declaration + usage = 2
        // Total = 6
        assert_eq!(
            len_str_count, 6,
            "Expected exactly 6 occurrences of _LEN_STR (3 decls + 3 usages).\nGenerated:\n{generated}"
        );
    }
}
