#!/usr/bin/env bash
set -euo pipefail

# Redis Manual Integration Test
#
# Tests the Redis Pub/Sub transport against a real Redis broker.
# Uses sensor_fullduplex to exercise concurrent subscribe serialization.
#
# Usage: ./redis.sh
# Example: ./redis.sh

FEATURE="transport_redis"
CONTAINER_NAME="mom-rpc-test-redis"
REDIS_PORT=6379
BROKER_URI="redis://localhost:${REDIS_PORT}"
: ${CLEAN_LOGS:=yes}
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

    # Stop and remove container
    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo "Stopping Redis container..."
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
        docker rm "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi

    # Remove temp files
    if [ "${CLEAN_LOGS}" == 'yes' ] ; then
        rm -f fullduplex.log ./*.build.log
    fi
}

trap cleanup EXIT INT TERM

# ---

echo "==> Starting Redis broker..."
echo "    Container: $CONTAINER_NAME"
echo "    Redis port: $REDIS_PORT"

# Remove existing container if present
docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true

docker run -d \
    --name "$CONTAINER_NAME" \
    -p "${REDIS_PORT}:6379" \
    redis:latest \
    >/dev/null

# Verify broker is accessible
if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo "Error: Redis broker failed to start properly"
    docker logs "$CONTAINER_NAME"
    exit 1
fi

echo "    ✓ Redis broker ready"

# ---

echo ""
echo "==> Building sensor_fullduplex with feature: $FEATURE"
echo
if ! cargo build -q --example sensor_fullduplex --features "$FEATURE" >& fullduplex.build.log; then
    echo "Error: Failed to build sensor_fullduplex"
    cat fullduplex.build.log
    exit 1
fi

echo "    ✓ Example built successfully"

# ---

echo ""
echo "==> Running sensor_fullduplex test..."

env BROKER_URI="${BROKER_URI}" RUST_LOG=info \
    cargo run -q --example sensor_fullduplex --features "$FEATURE" >& fullduplex.log

if grep -q  "Temperature" fullduplex.log && \
   grep -q  "Humidity"    fullduplex.log && \
   grep -q  "Pressure"    fullduplex.log ; then
    echo ""
    echo "✅ Redis integration test PASSED"
    echo ""
    echo "Feature tested: $FEATURE"
    echo "Broker URI: $BROKER_URI"
    echo "Output:"
    cat fullduplex.log
    exit 0
else
    echo ""
    echo "❌ Redis integration test FAILED"
    echo ""
    echo "Expected output containing Temperature / Humidity / Pressure"
    echo ""
    echo "fullduplex output:"
    cat fullduplex.log
    exit 1
fi
