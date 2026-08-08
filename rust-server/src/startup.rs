use crate::config::HOSTNAME;
use crate::ALL_ASSETS;

/// Format a byte count as a human-readable string (e.g. "1.2 KB").
fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit_idx])
    }
}

/// Prints the CLI help message to stderr and exits.
pub(crate) fn print_help() -> ! {
    elog!("Usage: app [--summary]");
    elog!();
    elog!("Options:");
    elog!("  --summary    Log aggregated req/s every 5s instead of per-request details");
    elog!("  --help, -h   Show this help message");
    elog!();
    elog!("Build-time environment variables (set at compile time):");
    elog!("  HOSTNAME              Server hostname (default: localhost)");
    elog!("  PORT                  Server port (default: 3000)");
    elog!("  WORKERS               Number of worker threads (default: available parallelism)");
    elog!("  MAX_CONNS             Maximum concurrent TCP connections (default: 1024)");
    elog!("  TCP_HANDLERS_PER_WORKER Number of TCP handler tasks per worker (default: max(MAX_CONNS/WORKERS, 64))");
    elog!("  H3_HANDLERS_PER_CONNECTION Number of h3 handler tasks per connection (default: 8)");
    elog!("  H2_CONN_WINDOW        HTTP/2 connection flow-control window in bytes (default: 16 MiB)");
    elog!("  H2_STREAM_WINDOW      HTTP/2 per-stream flow-control window in bytes (default: 4 MiB)");
    elog!("  H2_MAX_FRAME_SIZE     HTTP/2 max frame size in bytes (default: 65535)");
    elog!("  H2_MAX_SEND_BUF       HTTP/2 per-stream send buffer in bytes (default: 1 MiB)");
    elog!("  SHUTDOWN_TIMEOUT_SECS Graceful shutdown timeout in seconds (default: 3)");
    elog!("  DISABLE_SRI           Disable Subresource Integrity hashing (default: false)");
    elog!("  ALLOW_INLINE_STYLES   Allow 'unsafe-inline' in style-src for dynamic CSS (default: false)");
    elog!("  DISABLE_LOGGING       Compile out all stderr output (default: true)");
    std::process::exit(0);
}

/// Prints the startup banner to stderr showing the listening URLs,
/// asset count, worker count, protocol support, and optional logging mode.
pub(crate) fn print_banner(port: u16, num_workers: usize, num_assets: usize, summary_mode: bool) {
    elog!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    elog!("  http://{HOSTNAME}:{port}  |  https://{HOSTNAME}:{port}");
    elog!();
    elog!(
        "  {num_assets} assets  ·  {num_workers} workers  ·  SO_REUSEPORT  ·  h1.1 / h2 / h3",
    );
    if summary_mode {
        elog!("  Log: req/s reported every 5s");
    }
    elog!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

/// Prints a table of all embedded assets showing the route/file, plain size,
/// and the size after each compression algorithm (gzip, brotli, zstd).
pub(crate) fn print_assets_table() {
    elog!();
    elog!(
        "{:<36} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "Route / File", "Plain", "Gzip", "Brotli", "Zstd", "Best"
    );
    elog!("{:-<81}", "");

    for asset in ALL_ASSETS {
        // The "best" encoding is the smallest among all variants — this is what
        // a client with full Accept-Encoding support would receive.
        let best = asset
            .uncompressed_length
            .min(asset.gzip_length)
            .min(asset.brotli_length)
            .min(asset.zstd_length);

        elog!(
            "{:<36} {:>9} {:>9} {:>9} {:>9} {:>9}",
            asset.file,
            format_bytes(asset.uncompressed_length),
            format_bytes(asset.gzip_length),
            format_bytes(asset.brotli_length),
            format_bytes(asset.zstd_length),
            format_bytes(best),
        );
    }

    // ── Totals ─────────────────────────────────────────────────────
    let total_plain: usize = ALL_ASSETS.iter().map(|a| a.uncompressed_length).sum();
    let total_gzip: usize = ALL_ASSETS.iter().map(|a| a.gzip_length).sum();
    let total_brotli: usize = ALL_ASSETS.iter().map(|a| a.brotli_length).sum();
    let total_zstd: usize = ALL_ASSETS.iter().map(|a| a.zstd_length).sum();
    // Total of the best (smallest) encoding for each asset.
    let total_best: usize = ALL_ASSETS
        .iter()
        .map(|a| {
            a.uncompressed_length
                .min(a.gzip_length)
                .min(a.brotli_length)
                .min(a.zstd_length)
        })
        .sum();

    elog!("{:-<81}", "");
    elog!(
        "{:<36} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "TOTAL",
        format_bytes(total_plain),
        format_bytes(total_gzip),
        format_bytes(total_brotli),
        format_bytes(total_zstd),
        format_bytes(total_best),
    );
    if total_plain > 0 {
        let ratio = (total_best as f64 / total_plain as f64) * 100.0;
        elog!("Overall compression: {:.1}% of plain size", ratio);
    }
    elog!();
}
