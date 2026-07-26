/// Prints the CLI help message to stderr and exits.
pub(crate) fn print_help() -> ! {
    eprintln!("Usage: app [--summary]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --summary    Log aggregated req/s every 5s instead of per-request details");
    eprintln!("  --help, -h   Show this help message");
    eprintln!();
    eprintln!("Environment variables:");
    eprintln!("  PORT                  Server port (default: 3000)");
    eprintln!("  WORKERS               Number of worker threads (default: available parallelism)");
    eprintln!("  MAX_CONNS             Maximum concurrent TCP connections (default: 1024)");
    eprintln!("  SHUTDOWN_TIMEOUT_SECS Graceful shutdown timeout in seconds (default: 30)");
    std::process::exit(0);
}

/// Prints the startup banner to stderr showing the listening URLs,
/// asset count, worker count, protocol support, and optional logging mode.
pub(crate) fn print_banner(port: u16, num_workers: usize, num_assets: usize, summary_mode: bool) {
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!("  http://localhost:{port}  |  https://localhost:{port}");
    eprintln!();
    eprintln!(
        "  {num_assets} assets  ·  {num_workers} workers  ·  SO_REUSEPORT  ·  h1.1 / h2 / h3",
    );
    if summary_mode {
        eprintln!("  Log: req/s reported every 5s");
    }
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}
