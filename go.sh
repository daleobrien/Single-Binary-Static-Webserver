#!/usr/bin/env bash
set -e

# Build the React frontend with production settings
echo ">>> Building React app (production)..."
export NVM_DIR="$HOME/.nvm" && [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
pushd react-app > /dev/null && nvm use node && npm run build && popd > /dev/null

if [ "${1:-}" = "--docker" ]; then
    docker build -f "rust-server/Dockerfile" -t app-rust .
    docker run -p 3000:3000 --rm app-rust
else
    pushd rust-server > /dev/null

    export ALLOW_INLINE_STYLES=1

    cargo test && cargo build --release
    TARGET_DIR=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
    BIN="$TARGET_DIR/release/app"
    echo ""
    echo ">>> Binary size: $(ls -lh "$BIN" | awk '{print $5}')"
    echo ""
    # Stop any existing process
    lsof -ti :3000 | xargs kill
    exec "$BIN"
    popd > /dev/null
fi
