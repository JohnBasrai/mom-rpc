#!/usr/bin/env bash
set -euo pipefail

# DDS Manual Integration Test
#
# Tests DDS transport implementations. DDS is brokerless - peers discover
# each other automatically via RTPS multicast.
#
# Usage:   ./dds.sh <feature-flag>
# Example: ./dds.sh transport_dust_dds

FEATURE="${1:-}"
: "${DOMAIN_ID:=0}"
: "${TRANSPORT_URI:=dds:${DOMAIN_ID}}"
NO_CLEAN=
NO_CLEAN="${2:-}"

export BROKER_URI="$TRANSPORT_URI"

: "${FEATURE:=transport_dust_dds}"
# ---

echo "==> Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is required but not found"
    exit 1
fi

# ---

# shell-check does not model traps well
# shellcheck disable=SC2317
cleanup() {
    echo ""
    echo "==> Cleaning up..."

    # Kill server process
    if [ -n "${SERVER_PID:-}" ]; then
        echo "Killing server (PID: $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    if [ -n "${CLIENT_PID:-}" ]; then
        echo "Killing client (PID: $CLIENT_PID)..."
        kill "$CLIENT_PID" 2>/dev/null || true
        wait "$CLIENT_PID" 2>/dev/null || true
    fi

    # Remove temp files
    if [ -z "${NO_CLEAN}" ] ; then
        rm -f server.log client.log ./*.build.log
    fi
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
echo "==> Starting sensor_client..."

cargo --quiet run --example sensor_client --features "$FEATURE" >& client.log &
CLIENT_PID=$!

echo ""
echo "==> Starting sensor_server..."

cargo run --quiet --example sensor_server --features "$FEATURE" > server.log 2>&1 &
SERVER_PID=$!

echo "    Server PID: $SERVER_PID"
echo "    Transport URI: $TRANSPORT_URI"
echo "    Waiting for server to initialize and DDS discovery..."

check_is_alive() {
    local pid=$1
    local who=$2
    # Check if $who is running
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "Error: $who process died"
        echo "$who logs:"
        cat "${who}.log"
        exit 1
    fi
}

# Check if server is still running
sleep 1
check_is_alive "${SERVER_PID}" server

echo "    ✓ Server running"

echo "    Waiting for client to finish (timeout 20s)..."

SECONDS_WAITED=0
while kill -0 "${CLIENT_PID}" 2>/dev/null; do
    if [ "${SECONDS_WAITED}" -ge 20 ]; then
        echo "Client timed out pid:${CLIENT_PID}"
        exit 1
    fi
    sleep 1
    SECONDS_WAITED=$((SECONDS_WAITED + 1))
done

wait "${CLIENT_PID}"


# ---

if grep -q  "Temperature" client.log && \
   grep -q  "Humidity"    client.log && \
   grep -q  "Pressure"    client.log && \
   ! grep -q ERROR client.log; then
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
