#!/usr/bin/env bash
# CI script for formatting and linting

set -e

echo "=== Running cargo fmt check ==="
cargo fmt --all -- --check

echo ""
echo "=== Running clippy ==="
cargo clippy --all-targets --all-features -- -D warnings

echo ""
echo "✅ All checks passed!"
