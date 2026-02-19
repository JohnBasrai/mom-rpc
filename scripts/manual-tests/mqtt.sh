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
    echo "  2. Builds and runs sensor_fullduplex example"
    echo "  3. Validates output contains Temperature, Humidity, Pressure"
    echo "  4. Cleans up (stops container)"
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

# shellcheck disable=SC2317
cleanup() {
    echo ""
    echo "==> Cleaning up..."

    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo "Stopping Mosquitto container..."
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
        docker rm   "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
}

trap cleanup EXIT INT TERM

# ---

echo "==> Starting Mosquitto MQTT broker..."
echo "    Container: $CONTAINER_NAME"
echo "    MQTT port: $MQTT_PORT"

docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true

docker run -d \
    --name "$CONTAINER_NAME" \
    -p "${MQTT_PORT}:1883" \
    eclipse-mosquitto:latest \
    mosquitto -c /mosquitto-no-auth.conf \
    >/dev/null

if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo "Error: Mosquitto broker failed to start"
    docker logs "$CONTAINER_NAME"
    exit 1
fi

echo "    ✓ Mosquitto broker ready"

# ---

echo ""
echo "==> Building sensor_fullduplex with feature: $FEATURE"

if ! cargo build --example sensor_fullduplex --features "$FEATURE" 2>&1; then
    echo "Error: Failed to build sensor_fullduplex"
    exit 1
fi

echo "    ✓ Example built successfully"

# ---

echo ""
echo "==> Running sensor_fullduplex test..."

OUTPUT=$(env BROKER_URI="${BROKER_URI}" RUST_LOG=info \
    cargo run --example sensor_fullduplex --features "$FEATURE" 2>&1)

echo "$OUTPUT"

if echo "$OUTPUT" | grep -q "Temperature" && \
   echo "$OUTPUT" | grep -q "Humidity"    && \
   echo "$OUTPUT" | grep -q "Pressure"    ; then
    echo ""
    echo "✅ MQTT integration test PASSED"
    echo ""
    echo "Feature tested: $FEATURE"
    echo "Broker URI: $BROKER_URI"
    exit 0
else
    echo ""
    echo "❌ MQTT integration test FAILED"
    echo ""
    echo "Expected output containing Temperature, Humidity, and Pressure"
    exit 1
fi
