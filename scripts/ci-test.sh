#!/usr/bin/env bash
set -euo pipefail

FEATURE="${CI_FEATURE:-default}"
COVERAGE="${CI_COVERAGE:-0}"

echo "$0: ==> Running tests"
echo "$0:     feature = ${FEATURE}"
echo "$0:     coverage = ${COVERAGE}"

case "$FEATURE" in
    "default")
        cargo test
        ;;
    "no-default-features")
        echo "$0:     Testing with no default features"
        cargo build --no-default-features
        cargo test --no-default-features
    ;;
    "all-features")
        echo "$0:     Testing with all features"
        cargo build --all-features
        cargo test --all-features
        ;;
    *)
        echo "$0:     Testing feature $FEATURE"
        cargo test --features "$FEATURE"
        ;;
esac

# Coverage runs ONCE and is opt-in
if [ "$COVERAGE" = "1" ]; then
  echo "$0: ==> Running coverage tests"
  cargo test --features transport_rumqttc
fi

echo "$0: ✅ Tests OK"
