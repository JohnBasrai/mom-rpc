#!/usr/bin/env bash
set -euo pipefail

echo "==> Running lint checks"

cargo xfmt --check
cargo clippy --all-targets --all-features -- -D warnings

echo "==> Lint OK"
echo "✅ All checks passed!"
