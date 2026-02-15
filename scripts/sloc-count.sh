#!/usr/bin/env bash
# Generate SLOC table for README.md
# Run manually before each release, copy output into README

set -euo pipefail

# Get version from Cargo.toml (handles multiple spaces before '=')
VERSION=$(awk -F'"' '/^version/ {print $2; exit}' Cargo.toml)


echo "Transport SLOC breakdown for README.md"
echo "======================================="
echo ""
echo "| Transport | Feature Flag | Lines of Code | Use Case |"
echo "|:----------|:-------------|--------------:|:---------|"

# In-memory
sloc=$(tokei src/transport/memory.rs --output json | jq '.Rust.code')
printf "| In-memory | *(always available)* | %3d | Testing, single-process |\n" "$sloc"

# AMQP
sloc=$(tokei src/transport/amqp/lapin.rs --output json | jq '.Rust.code')
printf "| AMQP      | \`transport_lapin\`    | %3d | RabbitMQ, enterprise messaging |\n" "$sloc"

# MQTT
sloc=$(tokei src/transport/mqtt/rumqttc.rs --output json | jq '.Rust.code')
printf "| MQTT      | \`transport_rumqttc\`  | %3d | IoT, lightweight pub/sub |\n" "$sloc"

# DDS
sloc=$(tokei src/transport/dds/dust_dds.rs --output json | jq '.Rust.code')
printf "| DDS       | \`transport_dust_dds\` | %3d | Real-time, mission-critical |\n" "$sloc"

echo ""
core=$(tokei src/ --output json | jq '.Rust.code')
echo "*Core library: 761 lines. Total: $core lines. SLOC measured using tokei (crates.io methodology). As of v$VERSION.*"
echo ""
echo "Total project SLOC: $core"
