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
