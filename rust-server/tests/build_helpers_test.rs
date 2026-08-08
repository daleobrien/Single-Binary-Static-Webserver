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
    use tempfile::tempdir;

    /// Helper to build a minimal CodegenCtx for testing.
    fn minimal_ctx(out_dir: &str) -> CodegenCtx {
        let version_headers = vec![
            ("content-type".to_string(), "text/plain".to_string()),
            ("cache-control".to_string(), "no-cache".to_string()),
        ];
        let not_found_headers = vec![
            ("content-type".to_string(), "text/html".to_string()),
        ];
        CodegenCtx {
            out_dir: out_dir.to_string(),
            build_version: "test-version-hash".to_string(),
            assets: vec![AssetGen {
                const_prefix: "INDEX_HTML".to_string(),
                url_paths: vec!["/".to_string(), "/index.html".to_string()],
                status_code: 200,
            }],
            asset_header_indices: vec![0],
            header_sets: vec![
                vec![("content-type".to_string(), "text/html".to_string())],
                version_headers,
                not_found_headers,
            ],
            version_header_idx: 1,
            not_found_header_idx: 2,
            not_found_const_prefix: None,
            files: vec!["index.html".to_string()],
            has_404: false,
            uncompressed_lengths: vec![1024],
            version_uncompressed_len: 9,
            gzip_lengths: vec![500],
            brotli_lengths: vec![450],
            zstd_lengths: vec![430],
            version_gzip_len: 50,
            version_brotli_len: 30,
            version_zstd_len: 25,
        }
    }

    // ── Asset struct includes all variant body fields ─────────────

    #[test]
    fn asset_struct_includes_variant_body_fields() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let ctx = minimal_ctx(out_dir);
        codegen::generate(&ctx);

        let generated = std::fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        assert!(
            generated.contains("pub body: &'static [u8],"),
            "Asset struct must include body field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub body_gzip: &'static [u8],"),
            "Asset struct must include body_gzip field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub body_brotli: &'static [u8],"),
            "Asset struct must include body_brotli field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub body_zstd: &'static [u8],"),
            "Asset struct must include body_zstd field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub headers_identity: &'static [(&'static str, &'static str)],"),
            "Asset struct must include headers_identity field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub headers_gzip: &'static [(&'static str, &'static str)],"),
            "Asset struct must include headers_gzip field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub headers_brotli: &'static [(&'static str, &'static str)],"),
            "Asset struct must include headers_brotli field.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("pub headers_zstd: &'static [(&'static str, &'static str)],"),
            "Asset struct must include headers_zstd field.\nGenerated:\n{generated}"
        );
    }

    // ── All encoding body constants are emitted per asset ─────────

    #[test]
    fn asset_constants_include_all_encoding_bodies() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let ctx = minimal_ctx(out_dir);
        codegen::generate(&ctx);

        let generated = std::fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // Uncompressed body
        assert!(
            generated.contains("const INDEX_HTML_BODY: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/gzip/index.html.gz.raw\"));"),
            "Must embed uncompressed body.\nGenerated:\n{generated}"
        );
        // Gzip body
        assert!(
            generated.contains("const INDEX_HTML_BODY_GZIP: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/gzip/index.html.gz\"));"),
            "Must embed gzip body.\nGenerated:\n{generated}"
        );
        // Brotli body
        assert!(
            generated.contains("const INDEX_HTML_BODY_BROTLI: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/brotli/index.html.br\"));"),
            "Must embed brotli body.\nGenerated:\n{generated}"
        );
        // Zstd body
        assert!(
            generated.contains("const INDEX_HTML_BODY_ZSTD: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/zstd/index.html.zst\"));"),
            "Must embed zstd body.\nGenerated:\n{generated}"
        );

        // Lengths from the context
        assert!(
            generated.contains("const INDEX_HTML_UNCOMPRESSED_LEN: usize = 1024;"),
            "INDEX_HTML_UNCOMPRESSED_LEN must match context.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const INDEX_HTML_GZIP_LEN: usize = 500;"),
            "INDEX_HTML_GZIP_LEN must match context.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const INDEX_HTML_BROTLI_LEN: usize = 450;"),
            "INDEX_HTML_BROTLI_LEN must match context.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const INDEX_HTML_ZSTD_LEN: usize = 430;"),
            "INDEX_HTML_ZSTD_LEN must match context.\nGenerated:\n{generated}"
        );
    }

    // ── Version asset includes all variant bodies ─────────────────

    #[test]
    fn version_asset_includes_all_variant_bodies() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let ctx = minimal_ctx(out_dir);
        codegen::generate(&ctx);

        let generated = std::fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        assert!(
            generated.contains("const VERSION_BODY: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/gzip/v.txt.gz.raw\"));"),
            "Must embed version uncompressed body.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const VERSION_BODY_GZIP: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/gzip/v.txt.gz\"));"),
            "Must embed version gzip body.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const VERSION_BODY_BROTLI: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/brotli/v.txt.br\"));"),
            "Must embed version brotli body.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const VERSION_BODY_ZSTD: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/zstd/v.txt.zst\"));"),
            "Must embed version zstd body.\nGenerated:\n{generated}"
        );

        assert!(
            generated.contains("const VERSION_UNCOMPRESSED_LEN: usize = 9;"),
            "VERSION_UNCOMPRESSED_LEN must match context.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const VERSION_GZIP_LEN: usize = 50;"),
            "VERSION_GZIP_LEN must match context.\nGenerated:\n{generated}"
        );
    }

    // ── 404 fallback asset has all bodies pointing to same bytes ──

    #[test]
    fn not_found_asset_all_variants_same_body() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let ctx = minimal_ctx(out_dir);
        codegen::generate(&ctx);

        let generated = std::fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // All 404 body variants use the same inline bytes
        assert!(
            generated.contains("const NOT_FOUND_BODY: &[u8] = b\"<h1>404 - Not Found</h1>\";"),
            "NOT_FOUND_BODY must be the inline 404 HTML.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const NOT_FOUND_BODY_GZIP: &[u8] = b\"<h1>404 - Not Found</h1>\";"),
            "NOT_FOUND_BODY_GZIP must match NOT_FOUND_BODY.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const NOT_FOUND_BODY_BROTLI: &[u8] = b\"<h1>404 - Not Found</h1>\";"),
            "NOT_FOUND_BODY_BROTLI must match NOT_FOUND_BODY.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("const NOT_FOUND_BODY_ZSTD: &[u8] = b\"<h1>404 - Not Found</h1>\";"),
            "NOT_FOUND_BODY_ZSTD must match NOT_FOUND_BODY.\nGenerated:\n{generated}"
        );
    }

    // ── Asset instances initialize all variant body fields ────────

    #[test]
    fn asset_instances_initialize_variant_bodies() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let ctx = minimal_ctx(out_dir);
        codegen::generate(&ctx);

        let generated = std::fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // File asset instance
        assert!(
            generated.contains("body_gzip: INDEX_HTML_BODY_GZIP,"),
            "Asset instance must initialize body_gzip.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("body_brotli: INDEX_HTML_BODY_BROTLI,"),
            "Asset instance must initialize body_brotli.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("body_zstd: INDEX_HTML_BODY_ZSTD,"),
            "Asset instance must initialize body_zstd.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("headers_identity: INDEX_HTML_HEADERS_IDENTITY,"),
            "Asset instance must initialize headers_identity.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("headers_gzip: INDEX_HTML_HEADERS_GZIP,"),
            "Asset instance must initialize headers_gzip.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("headers_brotli: INDEX_HTML_HEADERS_BROTLI,"),
            "Asset instance must initialize headers_brotli.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("headers_zstd: INDEX_HTML_HEADERS_ZSTD,"),
            "Asset instance must initialize headers_zstd.\nGenerated:\n{generated}"
        );

        // Version asset instance
        assert!(
            generated.contains("body_gzip: VERSION_BODY_GZIP,"),
            "VERSION_ASSET must initialize body_gzip.\nGenerated:\n{generated}"
        );
        assert!(
            generated.contains("body_brotli: VERSION_BODY_BROTLI,"),
            "VERSION_ASSET must initialize body_brotli.\nGenerated:\n{generated}"
        );

        // 404 asset instance
        assert!(
            generated.contains("body_gzip: NOT_FOUND_BODY_GZIP,"),
            "NOT_FOUND_ASSET must initialize body_gzip.\nGenerated:\n{generated}"
        );
    }

    // ── Generated output does not reference removed fields ────────

    #[test]
    fn generated_output_has_no_len_str() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_str().unwrap();
        let ctx = minimal_ctx(out_dir);
        codegen::generate(&ctx);

        let generated = std::fs::read_to_string(format!("{out_dir}/generated.rs")).unwrap();

        // content_length as a struct field is removed — but "content-length"
        // as a header key in the precomputed header arrays is now expected.
        assert!(
            !generated.contains("content_length:"),
            "Generated code must not contain content_length: field (the struct field was removed).\nGenerated:\n{generated}"
        );
        // Verify the precomputed header arrays do contain content-length
        assert!(
            generated.contains("\"content-length\""),
            "Generated code must contain content-length in header arrays.\nGenerated:\n{generated}"
        );
    }
}
