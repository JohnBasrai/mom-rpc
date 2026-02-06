#!/usr/bin/env bash
set -euo pipefail

echo "==> Running lint checks"

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

echo "==> Lint OK"
echo "✅ All checks passed!"
