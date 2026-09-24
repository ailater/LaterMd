#!/usr/bin/env bash
# Local CI script, run before pushing.
set -euo pipefail

echo "--- cargo fmt --check ---"
cargo fmt --check

echo "--- clippy (default features) ---"
cargo clippy --all-targets -- -D warnings

echo "--- clippy (no default features) ---"
cargo clippy --all-targets --no-default-features -- -D warnings

echo "--- clippy (all features) ---"
cargo clippy --all-targets --all-features -- -D warnings

echo "--- cargo test ---"
cargo test --all-features

echo "--- cargo doc ---"
cargo doc --no-deps --all-features

echo "All checks passed."
