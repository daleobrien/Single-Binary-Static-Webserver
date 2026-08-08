mod build_helpers;

fn main() {
    // ── Declare custom cfg so `#[cfg(disable_http3)]` doesn't trigger warnings ──
    println!("cargo::rustc-check-cfg=cfg(disable_http3)");

    // ── Track build-time config env vars so cargo rebuilds when they change ──
    println!("cargo:rerun-if-env-changed=HOSTNAME");
    println!("cargo:rerun-if-env-changed=PORT");
    println!("cargo:rerun-if-env-changed=WORKERS");
    println!("cargo:rerun-if-env-changed=MAX_CONNS");
    println!("cargo:rerun-if-env-changed=TCP_HANDLERS_PER_WORKER");
    println!("cargo:rerun-if-env-changed=H3_HANDLERS_PER_CONNECTION");
    println!("cargo:rerun-if-env-changed=SHUTDOWN_TIMEOUT_SECS");
    println!("cargo:rerun-if-env-changed=H2_CONN_WINDOW");
    println!("cargo:rerun-if-env-changed=H2_STREAM_WINDOW");
    println!("cargo:rerun-if-env-changed=H2_MAX_FRAME_SIZE");
    println!("cargo:rerun-if-env-changed=H2_MAX_SEND_BUF");
    println!("cargo:rerun-if-env-changed=DISABLE_LOGGING");
    println!("cargo:rerun-if-env-changed=DISABLE_HTTP3");
    println!("cargo:rerun-if-env-changed=DISABLE_SRI");
    println!("cargo:rerun-if-env-changed=ALLOW_INLINE_STYLES");
    println!("cargo:rerun-if-env-changed=NOT_FOUND_FILENAME");
    println!("cargo:rerun-if-env-changed=EMBED_GZIP");
    println!("cargo:rerun-if-env-changed=EMBED_BROTLI");
    println!("cargo:rerun-if-env-changed=EMBED_ZSTD");

    // Rebuild when any file in public/ changes (recursive).
    println!("cargo:rerun-if-changed=../public");

    // Emit a cfg flag so the source code can conditionally compile h3 code.
    let disable_http3: bool = std::env::var("DISABLE_HTTP3")
        .ok()
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(true);
    if disable_http3 {
        println!("cargo:rustc-cfg=disable_http3");
    }

    build_helpers::run();
}
