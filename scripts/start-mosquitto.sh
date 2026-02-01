#!/usr/bin/env bash
# Start mosquitto MQTT broker for testing
#
# This script starts mosquitto with a simple configuration suitable for local testing.
# For production use, configure authentication, TLS, and access control.

set -e

echo "Starting mosquitto MQTT broker..."
echo "  Protocol: MQTT v5"
echo "  Port: 1883"
echo "  Listener: localhost"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Start mosquitto in verbose mode
# Adjust path if mosquitto is installed elsewhere
mosquitto -v -p 1883

# Alternative: Use docker
# docker run -it --rm --name mosquitto -p 1883:1883 eclipse-mosquitto:latest
