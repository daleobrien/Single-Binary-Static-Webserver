#!/usr/bin/env bash
set -e

docker build -f "rust-server/Dockerfile" -t app-rust .
docker run -p 3000:3000 --rm app-rust
