#!/usr/bin/env bash
set -euo pipefail

echo "Formatting..."
cargo fmt --all -- --check

echo "Clippy..."
cargo clippy --all-features -- -D warnings

echo "Tests..."
cargo test --all-features

echo "WASM check..."
cargo check --target wasm32-unknown-unknown --no-default-features

echo "All checks passed."
