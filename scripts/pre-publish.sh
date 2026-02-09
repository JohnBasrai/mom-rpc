#!/usr/bin/env bash
set -euo pipefail

echo "==> Pre-publish validation"
echo ""

echo "==> Packaging check..."
cargo package --allow-dirty

echo ""
echo "==> Dry-run publish to crates.io..."
cargo publish --dry-run --allow-dirty

echo ""
echo "✅ Ready to publish!"
echo "   Run: cargo publish"
