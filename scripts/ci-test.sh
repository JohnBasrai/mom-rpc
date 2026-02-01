#!/usr/bin/env bash
# CI script for running tests

set -e

echo "=== Running unit tests ==="
cargo test --lib

echo ""
echo "=== Running integration tests ==="
echo "NOTE: Requires mosquitto running on localhost:1883"
cargo test --test integration

echo ""
echo "=== Building examples ==="
cargo build --examples

echo ""
echo "✅ All tests passed!"
