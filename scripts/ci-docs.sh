#!/usr/bin/env bash
set -euo pipefail

echo "==> Checking documentation"

# Check that docs build without warnings
echo "    Building docs..."
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --quiet

# Check for missing docs on public items
echo "    Checking for missing documentation..."
RUSTDOCFLAGS="-D warnings" \
    cargo doc --all-features --no-deps 2>&1 | grep "warning.*missing documentation" && {
  echo "❌ Found undocumented public items"
  exit 1
} || true

# Verify examples compile (they should also serve as documentation)
echo "    Verifying examples compile..."
cargo build --example sensor_memory --quiet
cargo build --example sensor_server --features transport_rumqttc --quiet
cargo build --example sensor_client --features transport_rumqttc --quiet

echo "✅ Documentation OK"
echo "You may view the docs with command:"
echo "cargo doc --no-deps --all-features --open"
