#!/usr/bin/env bash
# CI script for running tests with multiple feature combinations

set -e

echo "=== Testing default features ==="
cargo test --all-targets

echo ""
echo "=== Testing all features ==="
cargo test --all-targets --all-features

echo ""
echo "=== Testing no default features ==="
cargo test --all-targets --no-default-features

echo ""
echo "=== Testing with transport_mqttac only ==="
cargo test --all-targets --no-default-features --features transport_mqttac

echo ""
echo "=== Building examples ==="
cargo build --examples --all-features

echo ""
echo "=== Running doc tests ==="
cargo test --doc --all-features

echo ""
echo "✅ All tests passed!"
