#!/usr/bin/env bash
set -euo pipefail

FEATURE="${CI_FEATURE:-default}"
COVERAGE="${CI_COVERAGE:-0}"

echo "==> Running tests"
echo "    feature = ${FEATURE}"
echo "    coverage = ${COVERAGE}"

if [ "$FEATURE" = "default" ]; then
  cargo test
else
  cargo test --features "$FEATURE"
fi

# Coverage runs ONCE and is opt-in
if [ "$COVERAGE" = "1" ]; then
  echo "==> Running coverage tests"
  cargo test --features transport_rumqttc
fi

echo "✅ Tests OK"
