#!/usr/bin/env bash
set -e

# Build the React frontend with production settings
if [ -d "react-app" ]; then
    echo ">>> Building React app (production)..."
    export NVM_DIR="$HOME/.nvm" && [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
    pushd react-app > /dev/null && nvm use node && npm run build && popd > /dev/null
fi

if [ "${1:-}" = "--docker" ]; then
    docker build -f "rust-server/Dockerfile" -t app-rust .
    docker run -p 3000:3000 --rm app-rust
else
    pushd rust-server > /dev/null
    # Run tests, show summary only; full output on failure
    if TEST_OUTPUT=$(cargo test 2>&1); then
        echo "$TEST_OUTPUT" | grep -E '(test result:|running )' || echo "✅ All tests passed."
    else
        echo "$TEST_OUTPUT"
        exit 1
    fi
    cargo build --release

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
