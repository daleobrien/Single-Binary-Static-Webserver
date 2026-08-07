use crate::config::HOSTNAME;

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
