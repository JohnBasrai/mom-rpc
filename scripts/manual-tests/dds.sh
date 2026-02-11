#!/usr/bin/env bash
set -euo pipefail

# DDS Manual Integration Test
#
# Tests DDS transport implementations. DDS is brokerless - peers discover
# each other automatically via RTPS multicast.
#
# Usage:   ./dds.sh <feature-flag>
# Example: ./dds.sh transport_rustdds

FEATURE="${1:-}"
DOMAIN_ID="${DDS_DOMAIN:-0}"
TRANSPORT_URI="dds:${DOMAIN_ID}"

# ---

usage() {
    echo "Usage: $0 <feature-flag>"
    echo ""
    echo "Example:"
    echo "  $0 transport_rustdds"
    echo ""
    echo "Environment variables:"
    echo "  DDS_DOMAIN - DDS domain ID (default: 0)"
    echo ""
    echo "This script:"
    echo "  1. Builds sensor_server and sensor_client examples"
    echo "  2. Runs sensor_server in background"
    echo "  3. Runs sensor_client and validates output"
    echo "  4. Cleans up (kills server)"
    echo ""
    echo "Note: DDS is brokerless - no external infrastructure needed"
    exit 1
}

if [ -z "$FEATURE" ]; then
    echo "Error: Feature flag required"
    usage
fi

# ---

echo "==> Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is required but not found"
    exit 1
fi

# ---

cleanup() {
    echo ""
    echo "==> Cleaning up..."
    
    # Kill server process
    if [ -n "${SERVER_PID:-}" ]; then
        echo "Killing server (PID: $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    
    # Remove temp files
    rm -f server.log client.log *.build.log
}

trap cleanup EXIT INT TERM

# ---

echo ""
echo "==> Building examples with feature: $FEATURE"
echo "    DDS domain: $DOMAIN_ID"

if ! cargo build --example sensor_server --features "$FEATURE" >& sensor_server.build.log; then
    echo "Error: Failed to build sensor_server"
    cat sensor_server.build.log
    exit 1
fi

if ! cargo build --example sensor_client --features "$FEATURE" >& sensor_client.build.log; then
    echo "Error: Failed to build sensor_client"
    cat sensor_client.build.log
    exit 1
fi

echo "    ✓ Examples built successfully"

# ---

echo ""
echo "==> Starting sensor_server..."

# Set transport URI via environment variable
export BROKER_URI="$TRANSPORT_URI"

cargo run --quiet --example sensor_server --features "$FEATURE" > server.log 2>&1 &
SERVER_PID=$!

echo "    Server PID: $SERVER_PID"
echo "    Transport URI: $TRANSPORT_URI"
echo "    Waiting for server to initialize and DDS discovery..."
sleep 3

# Check if server is still running
if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "Error: Server process died"
    echo "Server logs:"
    cat server.log
    exit 1
fi

echo "    ✓ Server running"

# ---

echo ""
echo "==> Running sensor_client..."

cargo --quiet run --example sensor_client --features "$FEATURE" >& client.log

if grep -q  "Temperature" client.log && \
   grep -q  "Humidity"    client.log && \
   grep -q  "Pressure"    client.log ; then
    echo ""
    echo "✅ DDS integration test PASSED"
    echo ""
    echo "Feature tested: $FEATURE"
    echo "DDS domain: $DOMAIN_ID"
    echo "Output:"
    cat  client.log
    exit 0
else
    echo ""
    echo "❌ DDS integration test FAILED"
    echo ""
    echo "Expected output containing Temperature, Humidity, and Pressure"
    echo ""
    echo "Client output:"
    cat client.log
    echo ""
    echo "Server logs:"
    cat server.log
    exit 1
fi
