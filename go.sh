#!/usr/bin/env bash
set -e

if [ "${1:-}" = "--docker" ]; then
    docker build -f "rust-server/Dockerfile" -t app-rust .
    docker run -p 3000:3000 --rm app-rust
else
    cd rust-server && cargo test && cargo run --release
fi
