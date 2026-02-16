#!/usr/bin/env bash
# Generate SLOC table for README.md
# Run manually before each release, copy output into README

set -euo pipefail

# Get version from Cargo.toml (handles multiple spaces before '=')
VERSION=$(awk -F'"' '/^version/ {print $2; exit}' Cargo.toml)

echo "Transport SLOC breakdown for README.md"
echo "======================================="
echo ""
echo "| Transport | Feature Flag | SLOC | Use Case |"
echo "|:----------|:-------------|-----:|:---------|"

# In-memory
memory_sloc=$(tokei src/transport/memory.rs --output json | jq '.Rust.code')
printf "| In-memory | *(always available)* | %3d | Testing, single-process |\n" "$memory_sloc"

# AMQP
amqp_sloc=$(tokei src/transport/amqp/lapin.rs --output json | jq '.Rust.code')
printf "| AMQP      | \`transport_lapin\`    | %3d | RabbitMQ, enterprise messaging |\n" "$amqp_sloc"

# MQTT
mqtt_sloc=$(tokei src/transport/mqtt/rumqttc.rs --output json | jq '.Rust.code')
printf "| MQTT      | \`transport_rumqttc\`  | %3d | IoT, lightweight pub/sub |\n" "$mqtt_sloc"

# DDS
dds_sloc=$(tokei src/transport/dds/dust_dds.rs --output json | jq '.Rust.code')
printf "| DDS       | \`transport_dust_dds\` | %3d | Real-time, mission-critical |\n" "$dds_sloc"

echo ""

# Calculate totals
total=$(tokei src/ --output json | jq '.Rust.code')
core=$((total - amqp_sloc - mqtt_sloc - dds_sloc))

echo "> *Core library: $core lines, including In-memory. Total: $total lines (as of v$VERSION).*"
echo "> *SLOC measured using \`tokei\` (crates.io methodology).*"
echo ">"
echo "> Example: An application using only the MQTT transport compiles $core + $mqtt_sloc = $((core + mqtt_sloc)) lines of \`mom-rpc\` code."
echo "> With both MQTT and AMQP enabled: $core + $mqtt_sloc + $amqp_sloc = $((core + mqtt_sloc + amqp_sloc)) lines."
