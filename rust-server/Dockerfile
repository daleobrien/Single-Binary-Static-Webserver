FROM rust:1.97.1-alpine AS builder

# musl-dev needed for fully static linking with the musl target
# gzip needed by build.rs for pre-compressing static assets
# upx for binary compression
RUN apk add --no-cache musl-dev gzip upx

# Map Docker's TARGETARCH to the Rust target triplet
# amd64 -> x86_64-unknown-linux-musl, arm64 -> aarch64-unknown-linux-musl
ARG TARGETARCH
RUN if [ "$TARGETARCH" = "arm64" ]; then \
        RUST_TARGET=aarch64-unknown-linux-musl; \
    else \
        RUST_TARGET=x86_64-unknown-linux-musl; \
    fi && \
    rustup target add "$RUST_TARGET" && \
    echo "$RUST_TARGET" > /rust_target

WORKDIR /app
COPY rust-server/ ./rust-server/
COPY public/ ./public/
COPY certs/ ./certs/

WORKDIR /app/rust-server

# build.rs handles gzip pre-compression automatically.
# Size optimizations are in Cargo.toml [profile.release]
# Build a fully static binary — no runtime dependencies at all
ENV DISABLE_LOGGING=false
ENV NOT_FOUND_FILENAME=index.html
ENV ALLOW_INLINE_STYLES=true
ENV EMBED_ZSTD=false
ENV EMBED_GZIP=false
ENV DISABLE_HTTP3=true

RUN RUST_TARGET=$(cat /rust_target) && \
    cargo build --release --target "$RUST_TARGET" && \
    cp "./target/$RUST_TARGET/release/app" /app-bin && \
    upx --best /app-bin

# Truly minimal: scratch (~0 MB) — only our static binary
FROM scratch

COPY --from=builder /app-bin /app

EXPOSE 3000
ENTRYPOINT ["/app"]
