#!/usr/bin/env bash
set -euo pipefail

# RabbitMQ Manual Integration Test
#
# Tests AMQP transport implementations against a real RabbitMQ broker.
# Multiple AMQP libraries can use the same script by passing different feature flags.
#
# Usage: ./rabbitmq.sh <feature-flag>
# Example: ./rabbitmq.sh transport_lapin

FEATURE="${1:-}"
CONTAINER_NAME="mom-rpc-test-rabbitmq"
AMQP_PORT=5672
MGMT_PORT=15672
BROKER_URI="amqp://localhost:${AMQP_PORT}/%2f"

# ---

usage() {
    echo "Usage: $0 <feature-flag>"
    echo ""
    echo "Example:"
    echo "  $0 transport_lapin"
    echo ""
    echo "This script:"
    echo "  1. Starts a RabbitMQ broker in Docker"
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
        echo "Stopping RabbitMQ container..."
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
        docker rm "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
    
    # Remove temp files
    rm -f server.log client.log *.build.log
}

trap cleanup EXIT INT TERM

# ---

echo "==> Starting RabbitMQ broker..."
echo "    Container: $CONTAINER_NAME"
echo "    AMQP port: $AMQP_PORT"
echo "    Management UI: http://localhost:$MGMT_PORT (guest/guest)"

# Remove existing container if present
docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true

# Start RabbitMQ with management UI
docker run -d \
    --name "$CONTAINER_NAME" \
    -p "${AMQP_PORT}:5672" \
    -p "${MGMT_PORT}:15672" \
    rabbitmq:3-management \
    >/dev/null

echo "    Waiting for broker to be ready..."
sleep 5

# Verify broker is accessible
if ! docker exec "$CONTAINER_NAME" rabbitmqctl status >/dev/null 2>&1; then
    echo "Error: RabbitMQ broker failed to start properly"
    docker logs "$CONTAINER_NAME"
    exit 1
fi

echo "    ✓ RabbitMQ broker ready"

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

cargo --quiet run --example sensor_client --features "$FEATURE" 2>&1 |& tee client.log

if grep -q  "Temperature" client.log && \
   grep -q  "Humidity"    client.log && \
   grep -q  "Pressure"    client.log  ; then
    echo ""
    echo "✅ RabbitMQ integration test PASSED"
    echo ""
    echo "Feature tested: $FEATURE"
    echo "Broker URI: $BROKER_URI"
    echo "Output:"
    cat  client.log
    exit 0
else
    echo ""
    echo "❌ RabbitMQ integration test FAILED"
    echo ""
    echo "Expected output not found"
    echo ""
    echo "Client output:"
    cat client.log
    echo ""
    echo "Server logs:"
    cat server.log
    exit 1
fi
