use criterion::{black_box, criterion_group, criterion_main, Criterion};

use app::{route, response_for_asset, Asset};

/// Benchmark the `route` function: path → &'static Asset lookup via the
/// compile-time-generated match statement.
fn bench_route(c: &mut Criterion) {
    let mut group = c.benchmark_group("route");

    // Static index.html
    group.bench_function("route /", |b| {
        b.iter(|| route(black_box("/")));
    });

    // Extensionless path (→ .html)
    group.bench_function("route /about (extensionless)", |b| {
        b.iter(|| route(black_box("/about")));
    });

    // /version endpoint
    group.bench_function("route /v", |b| {
        b.iter(|| route(black_box("/v")));
    });

    // 404 fallback (catch-all arm)
    group.bench_function("route /nonexistent (404)", |b| {
        b.iter(|| route(black_box("/nonexistent/path")));
    });

    group.finish();
}

/// Benchmark `response_for_asset`: constructing a `hyper::Response<Full<Bytes>>`
/// from a pre-built `&'static Asset`. This exercises the header-insertion path
/// (HeaderName::from_static / HeaderValue::from_static) using the compile-time
/// static header slices — specifically verifying that the optimisation which
/// removed HeaderMap::clone() does not regress.
fn bench_response_for_asset(c: &mut Criterion) {
    let index_asset: &Asset = route("/");
    let not_found_asset: &Asset = route("/nonexistent/path");
    let version_asset: &Asset = route("/v");

    let mut group = c.benchmark_group("response_for_asset");

    // Small asset (index.html)
    group.bench_function("index.html", |b| {
        b.iter(|| {
            let resp = response_for_asset(black_box(index_asset));
            black_box(resp)
        });
    });

    // 404 fallback (small inline body)
    group.bench_function("404 fallback", |b| {
        b.iter(|| {
            let resp = response_for_asset(black_box(not_found_asset));
            black_box(resp)
        });
    });

    // Version asset
    group.bench_function("/v", |b| {
        b.iter(|| {
            let resp = response_for_asset(black_box(version_asset));
            black_box(resp)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_route, bench_response_for_asset);
criterion_main!(benches);
