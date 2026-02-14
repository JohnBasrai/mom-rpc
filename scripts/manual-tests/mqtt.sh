#!/usr/bin/env bash
set -euo pipefail

# MQTT Manual Integration Test
#
# Tests MQTT transport implementations against a real Mosquitto broker.
# Multiple MQTT libraries can use the same script by passing different feature flags.
#
# Usage: ./mqtt.sh <feature-flag>
# Example: ./mqtt.sh transport_rumqttc

FEATURE="${1:-}"
CONTAINER_NAME="mom-rpc-test-mosquitto"
MQTT_PORT=1883
BROKER_URI="mqtt://localhost:${MQTT_PORT}"
: "${FEATURE:=transport_rumqttc}"
# ---

usage() {
    echo "Usage: $0 <feature-flag>"
    echo ""
    echo "Example:"
    echo "  $0 transport_rumqttc"
    echo ""
    echo "This script:"
    echo "  1. Starts a Mosquitto MQTT broker in Docker"
    echo "  2. Builds and runs sensor_server example"
    echo "  3. Runs sensor_client example and validates output"
    echo "  4. Cleans up (kills server, stops container)"
    exit 1
}

if [ -z "$FEATURE" ]; then
    echo "Error: Feature flag required"
    usage
fi

# ---

echo "==> Checking prerequisites..."

if ! command -v docker &> /dev/null; then
    echo "Error: Docker is required but not found"
    echo "Install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

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
    
    # Stop and remove container
    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo "Stopping Mosquitto container..."
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
        docker rm "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
    
    # Remove temp files
    rm -f server.log client.log ./*.build.log
}

trap cleanup EXIT INT TERM

# ---

echo "==> Starting Mosquitto MQTT broker..."
echo "    Container: $CONTAINER_NAME"
echo "    MQTT port: $MQTT_PORT"

# Remove existing container if present
docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true

# Start Mosquitto with anonymous access enabled for testing
docker run -d \
    --name "$CONTAINER_NAME" \
    -p "${MQTT_PORT}:1883" \
    eclipse-mosquitto:latest \
    mosquitto -c /mosquitto-no-auth.conf \
    >/dev/null

echo "    Waiting for broker to be ready..."
sleep 2

# Verify broker is accessible by checking if container is running
if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo "Error: Mosquitto broker failed to start properly"
    docker logs "$CONTAINER_NAME"
    exit 1
fi

echo "    ✓ Mosquitto broker ready"

# ---

echo ""
echo "==> Building examples with feature: $FEATURE"

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

# Set broker URI via environment variable (examples should read this)
export BROKER_URI="$BROKER_URI"

cargo run --quiet --example sensor_server --features "$FEATURE" > server.log 2>&1 &
SERVER_PID=$!

echo "    Server PID: $SERVER_PID"
echo "    Waiting for server to initialize..."
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
    echo "✅ MQTT integration test PASSED"
    echo ""
    echo "Feature tested: $FEATURE"
    echo "Broker URI: $BROKER_URI"
    echo "Output:"
    cat  client.log
    exit 0
else
    echo ""
    echo "❌ MQTT integration test FAILED"
    echo ""
    echo "Expected output containing \"${PATTERN}\" but didn't find it"
    echo ""
    echo "Client output:"
    cat client.log
    echo ""
    echo "Server logs:"
    cat server.log
    exit 1
fi
