mod response;

// ── Compile-time generated assets ──────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

// Re-export items needed by benchmarks and external tests.
pub use response::response_for_asset;
// route, Asset, BUILD_VERSION are already pub from generated.rs
