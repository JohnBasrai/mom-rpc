#!/usr/bin/env bash
set -euo pipefail

echo "==> Pre-publish validation"
echo ""

echo "==> Git checks..."

# Ensure on main branch
branch=$(git rev-parse --abbrev-ref HEAD)
if [[ "$branch" != "main" ]]; then
    echo "ERROR: Not on main branch (current: $branch)"
    exit 1
fi

# Ensure working tree clean
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "ERROR: Working tree is not clean"
    git status --short
    exit 1
fi

# Ensure no unpushed commits
git fetch origin main >/dev/null 2>&1
if [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]]; then
    echo "ERROR: Local main is not in sync with origin/main"
    exit 1
fi

echo "✓ Git state OK"
echo ""

echo "==> Packaging check..."
cargo package

echo ""
echo "==> Dry-run publish to crates.io..."
cargo publish --dry-run

echo ""
echo "✅ Ready to publish!"
echo "   Run: cargo publish"
