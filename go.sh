#!/usr/bin/env bash
set -e

if [ "${1:-}" = "--docker" ]; then
    docker build -f "rust-server/Dockerfile" -t app-rust .
    docker run -p 3000:3000 --rm app-rust
else
    cd rust-server && cargo test && cargo build --release
    TARGET_DIR=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
    BIN="$TARGET_DIR/release/app"
    echo ""
    echo ">>> Binary size: $(ls -lh "$BIN" | awk '{print $5}')"
    echo ""
    exec "$BIN"
fi
