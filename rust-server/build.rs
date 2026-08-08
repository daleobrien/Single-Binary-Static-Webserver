mod build_helpers;

fn main() {
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
    println!("cargo:rerun-if-env-changed=DISABLE_SRI");
    println!("cargo:rerun-if-env-changed=ALLOW_INLINE_STYLES");
    println!("cargo:rerun-if-env-changed=NOT_FOUND_FILENAME");
    println!("cargo:rerun-if-env-changed=EMBED_GZIP");
    println!("cargo:rerun-if-env-changed=EMBED_BROTLI");
    println!("cargo:rerun-if-env-changed=EMBED_ZSTD");

    // Rebuild when any file in public/ changes (recursive).
    println!("cargo:rerun-if-changed=../public");

    build_helpers::run();
}
