/// Compile-time-guarded `eprintln!` — when `DISABLE_LOGGING` is true at
/// build time, all output is compiled out (zero runtime cost).
macro_rules! elog {
    ($($arg:tt)*) => {
        if !$crate::config::DISABLE_LOGGING {
            eprintln!($($arg)*);
        }
    };
}

mod config;
mod error;
mod handlers;
mod logging;
mod quic_worker;
mod shutdown;
mod sockets;
mod startup;
mod tcp_worker;
mod tls_stream;

// ── Compile-time generated assets ──────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

// Re-export items needed by benchmarks and external tests.
pub use handlers::response_for_asset;
// route, Asset, BUILD_VERSION are already pub from generated.rs
