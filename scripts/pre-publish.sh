#!/usr/bin/env bash
set -euo pipefail
echo "==> Packaging validation"
cargo package --allow-dirty
cargo publish --dry-run --allow-dirty --registry crates-io
echo "✅ Ready to publish: cargo publish"
