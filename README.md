<h1><picture>
    <source srcset="/public/favicon.svg" type="image/svg+xml">
    <img src="/public/favicon.svg" width="36" height="36" alt="" align="top" />
  </picture>
  Static - A very fast standalone static file web server
</h1>

A minimal, self-contained web server written in Rust that is built on [Hyper](https://hyper.rs/), [rustls](https://github.com/rustls/rustls), and [quinn](https://github.com/quinn-rs/quinn) that serves HTML, CSS, JavaScript, and other static files — all minified then gzip-compressed at build time and embedded directly in a single binary. Zero runtime dependencies, zero disk I/O, zero runtime compression.

## Features

- **Single static binary** — all assets are minified, gzip-compressed, and embedded at compile time; no filesystem reads at runtime
- **Fully compile-time configuration** — all settings (hostname, port, worker count, connection limits, etc.) are baked into the binary by `build.rs`; zero environment variable reads at runtime
- **Content-hashed filenames** — every JS, CSS, and HTML file gets a content-hashed URL (e.g. `script.a8f2c3d.js`) with Subresource Integrity (SRI) hashes injected into HTML files, enabling aggressive cache headers with immutable fingerprints
- **Auto-reloading clients** — every HTML response includes a tiny inline polling script that checks a build-version endpoint (`/v`), automatically refreshing the page when a new version is deployed
- **Extensionless URL resolution** — `/about` serves `about.html`; `/` serves `index.html`
- **Custom 404 page** — place a `404.html` in `public/` and it's served for all unmatched routes
- **HTTP/1.1, HTTP/2 (h2), and HTTP/3 (h3 over QUIC)** — all on a single port with automatic protocol detection
- **Auto-detected TLS** — the first byte of each TCP connection is inspected: TLS ClientHello (`0x16`) triggers a TLS handshake, otherwise plain HTTP is served; both coexist on the same port
- **Flexible TLS certificates** — provide `certs/cert.pem` and `certs/key.pem` for custom certs, or let the build auto-generate a self-signed certificate for the configured hostname
- **Column-aligned, buffered request logging** — logs are collected and flushed once per second, showing protocol, method, path, status, size, and response time in microseconds
- **Summary logging mode** — run with `--summary` for a lightweight req/s counter updated every 5 seconds (ideal for benchmarks)
- **SO_REUSEPORT** — one TCP listener and one QUIC endpoint per worker; the kernel distributes connections across CPU cores with no accept-queue bottleneck and no shared locks
- **Security headers** — every response includes a strong set of HTTP security headers (CSP, HSTS, X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy, etc.) with SRI hashes on `<script>` and `<link>` tags.

## How It Works

### Build time (`build.rs`)

At compile time, `build.rs` processes every file in `../public/`:

1. **Configuration generation** — `config_gen.rs` reads all server configuration from environment variables and emits a `config_constants.rs` file containing `const` items (`HOSTNAME`, `PORT`, `NUM_WORKERS`, `MAX_CONNECTIONS`, `SHUTDOWN_TIMEOUT_SECS`). These are included directly into the runtime code and baked into the binary.

2. **Minification** — each file is minified using Rust crates:
   - HTML → [`minify-html`](https://crates.io/crates/minify-html) (with inline CSS/JS minification)
   - CSS → [`css-minify`](https://crates.io/crates/css-minify) (Level 3 optimizations)
   - JavaScript → [`minify-js`](https://crates.io/crates/minify-js)

3. **Content hashing & SRI** — SHA-256 digests are computed for each file:
   - Content-hashed filenames are generated (e.g. `script.a1b2c3d4.js`)
   - SRI `integrity` attributes are injected into `<script>` and `<link>` tags in HTML files
   - A small auto-reload polling script is injected right before `</body>`

4. **Gzip compression** — each file is compressed with [`flate2`](https://crates.io/crates/flate2) at max compression. Both the compressed (`.gz`) and uncompressed (`.gz.raw`) versions are kept; the server chooses whichever is smaller at build time.

5. **TLS certificate** — `build.rs` either converts provided PEM certificates to DER or generates a self-signed certificate using [`rcgen`](https://crates.io/crates/rcgen), embedding both into the binary.

6. **Code generation** — `generated.rs` is emitted to `OUT_DIR/` containing:
   - `&[u8]` constants for every asset via `include_bytes!`
   - `fn build_headers_N() -> HeaderMap` functions that construct headers with `HeaderName::from_static` / `HeaderValue::from_static` — no byte parsing, just direct insertion calls the compiler can optimize
   - Deduplicated header sets across assets sharing the same headers
   - A `route(path) -> &Asset` match function mapping URL paths to assets
   - `ALL_ASSETS`, `MAX_PATH_LEN`, `MAX_SIZE_DIGITS`, and `build_tls_config()`

### Runtime

`main.rs` includes the generated source at compile time via `include!(concat!(env!("OUT_DIR"), "/generated.rs"))`. All asset bodies, MIME types, header sets, and the route table are `const`/`static` data baked into the binary. Likewise, `config.rs` includes the generated `config_constants.rs` for all server settings.

At startup, each header-builder function is called exactly once and cached in a `LazyLock<Vec<HeaderMap>>`. Each request then does a zero-alloc route lookup, clones a pre-built `HeaderMap`, and sends the gzipped body.

**Per-request flow:**
1. Route lookup via `route(path)` — a `match` on the path string
2. Clone the pre-built `HeaderMap` (shared, immutable headers)
3. Return the embedded `&'static [u8]` body bytes with correct `Content-Type`, `Content-Encoding`, `Cache-Control`, and security headers

## Project Structure

```
├── certs/           # Optional custom TLS certificates
├── public/          # Your static assets (HTML, CSS, JS, SVG, etc.)
├── rust-server/     # The Rust server
│   ├── Cargo.toml
│   ├── Dockerfile
│   ├── build.rs     # Build script (config, minification, SRI, codegen, TLS)
│   ├── build_helpers/
│   └── src/         # Runtime source code
├── go.sh            # One-liner to either run docker or run locally
└── README.md
```

## Configuration

All configuration is resolved at **build time** by `build.rs` and baked into the binary. The server performs zero environment variable reads and zero config file I/O at runtime.

Set variables on the command line when building, or persist them in `.cargo/config.toml`:

```toml
[env]
HOSTNAME = "myhost.local"
PORT = "8080"
WORKERS = "4"
MAX_CONNS = "2048"
SHUTDOWN_TIMEOUT_SECS = "60"
NOT_FOUND_FILENAME = "not-found.html"
DISABLE_SRI = "true"
```

| Variable | Default | Description |
| --- | --- | --- |
| `HOSTNAME` | `localhost` | Hostname displayed in the startup banner and used in auto-generated TLS certificate SANs |
| `PORT` | `3000` | Server port (TCP and UDP) |
| `WORKERS` | *available parallelism* (floor 4) | Number of worker threads (one TCP listener and QUIC endpoint each) |
| `MAX_CONNS` | `4096` | Maximum concurrent TCP connections (enforced via a shared semaphore) |
| `TCP_HANDLERS_PER_WORKER` | `max(MAX_CONNS / WORKERS, 64)` | Number of pre-spawned TCP handler tasks per worker |
| `H3_HANDLERS_PER_CONNECTION` | `8` | Number of pre-spawned h3 handler tasks per QUIC connection |
| `H2_CONN_WINDOW` | `16777216` (16 MiB) | HTTP/2 connection-level flow-control window in bytes
| `H2_STREAM_WINDOW` | `4194304` (4 MiB) | HTTP/2 per-stream flow-control window in bytes
| `H2_MAX_FRAME_SIZE` | `65535` | HTTP/2 max frame size (range: 16384–16777215)
| `H2_MAX_SEND_BUF` | `1048576` (1 MiB) | HTTP/2 max per-stream write buffer before backpressure
| `SHUTDOWN_TIMEOUT_SECS` | `30` | Graceful shutdown timeout — how long to wait for in-flight requests after SIGINT/SIGTERM |
| `NOT_FOUND_FILENAME` | `404.html` | Name of the file in `public/` used as the custom 404 page |
| `DISABLE_SRI` | `false` | Set to `1` or `true` to disable Subresource Integrity (content-hashed filenames, `integrity` attributes in HTML, and CSP hash allowlisting). When disabled, CSP uses `'self' 'unsafe-inline'` for scripts and styles to allow the inline version-check script. |
| `DISABLE_LOGGING` | `true` | Compile out all stderr output (request logs, error messages, startup banner, and `--help` text). Set to `false` to re-enable logging. |

In addition to the user-configurable variables above, the build consumes two environment variables that are set automatically and are not meant to be configured manually:

| Variable | Set by | Purpose |
| --- | --- | --- |
| `OUT_DIR` | Cargo | Output directory for the build script; `build.rs` writes `generated.rs` and `config_constants.rs` there, and the runtime reads them back via `include!`/`include_bytes!` |
| `TARGETARCH` | Docker | BuildKit-provided build arg that selects the musl target triplet (`x86_64-unknown-linux-musl` vs `aarch64-unknown-linux-musl`) in the Dockerfile |

## Build & Run

> **Note:** Local builds use `target.tmp` as the Cargo target directory (instead of the default `target/`) to avoid conflicts with Docker builds. The `target.tmp/` directory is gitignored.

### Local

Default build (listens on `localhost:3000` with auto-detected parallelism):

```sh
cd rust-server
cargo run
```

Custom configuration via build-time env vars:

```sh
PORT=8080 WORKERS=4 cargo run
```

CLI flags (run `cargo run -- --help` for the full list):

```sh
cargo run -- --summary    # Log aggregated req/s every 5s instead of per-request details
```

The server listens on the configured hostname and port:
- `http://localhost:3000/` — plain HTTP
- `https://localhost:3000/` — TLS (HTTP/1.1, HTTP/2, or HTTP/3)

### Docker

The Docker build cross-compiles a fully static binary with musl, then compresses it with [UPX](https://upx.github.io/) for minimal image size (`FROM scratch`, ~1.09 MB). A self-signed TLS certificate is generated at build time (or picked up from `certs/` if present).

```sh
docker build -f rust-server/Dockerfile -t app-rust .
docker run -p 3000:3000 --rm app-rust
```

The Dockerfile's only build arg is `TARGETARCH`, which Docker sets automatically (see the configuration table above). The server's build-time env vars are **not** forwarded by the current Dockerfile — to bake them in, declare ARGs and pass them to the `cargo build` step:

```dockerfile
ARG HOSTNAME=myhost.local
ARG PORT=8080
# ... then use: HOSTNAME=$HOSTNAME PORT=$PORT cargo build --release --target "$RUST_TARGET"
```

Or simply:

```sh
./go.sh
```

## Protocol Detection

A single TCP port (3000 by default) serves plain HTTP, TLS HTTP/1.1, TLS HTTP/2, and HTTP/3 (QUIC on UDP). Protocol detection works as follows:

- **TCP connections** — the first byte is peeked. If it's `0x16` (TLS `ContentType::Handshake`), a TLS handshake is performed and Hyper's `auto::Builder` negotiates HTTP/1.1 or HTTP/2 via ALPN. Otherwise the connection is treated as plain HTTP.
- **UDP (QUIC)** — a separate QUIC endpoint on the same port handles HTTP/3 connections independently.

## Logging

### Detailed mode (default)

Logs are batched and flushed once per second with column-aligned output:

```
PR  METHOD   PATH            STA  SIZE  TIME
h2  GET      /                  200   123B  76µs
h1  GET      /about             200   456B  89µs
h3  GET      /style.a1b2.js     200   789B  49µs
```

### Summary mode (`--summary`)

A lightweight counter that prints req/s every 5 seconds:

```
127 requests in the last 5s (25.4 req/s)
```

## Version Check & Auto-Reload

Every HTML response includes a small inline script that periodically calls `GET /v`. The `/v` endpoint returns the build version hash (an opaque fingerprint of all files in `public/`) with an `ETag`. When a new build is deployed with changed content, the version hash changes, and all open browser tabs automatically reload.

The version check uses `If-None-Match` for efficient 304 responses when the version hasn't changed.

## License

MIT
