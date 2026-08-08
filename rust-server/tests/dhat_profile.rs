// Allocation profiling with dhat.
//
// Run with:
//   cargo test --test dhat_profile -- profile_allocations --nocapture
//
// This produces a `dhat-heap.json` file that can be viewed in the
// dhat viewer (https://nnethercote.github.io/dh_view/dh_view.html).

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn profile_allocations() {
    let _profiler = dhat::Profiler::new_heap();

    // ── Exercise the request path ──────────────────────────────

    // Route lookup: path → &'static Asset via compile-time match.
    for _ in 0..1000 {
        let _asset = app::route("/");
        let _asset = app::route("/v");
        let _asset = app::route("/nonexistent");
    }

    // Response construction: header insertion from static slices.
    // This is the key path that was optimised to avoid HeaderMap::clone().
    let index_asset = app::route("/");
    let not_found_asset = app::route("/nonexistent");
    let encoding = app::ContentEncoding::Brotli;

    for _ in 0..1000 {
        let _resp = app::response_for_asset(index_asset, encoding);
    }
    for _ in 0..1000 {
        let _resp = app::response_for_asset(not_found_asset, encoding);
    }

    // Build version ETag check path
    for _ in 0..100 {
        let v = app::BUILD_VERSION;
        let _ = v.len();
    }

    // _profiler drops here — writes dhat-heap.json
    eprintln!("dhat-heap.json written — open in https://nnethercote.github.io/dh_view/dh_view.html");
}
