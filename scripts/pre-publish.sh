#!/usr/bin/env bash
set -euo pipefail

echo "================================================"
echo "Pre-Publication Verification for crates.io"
echo "================================================"
echo ""

# Function to run a check and track results
CHECKS_PASSED=0
CHECKS_FAILED=0

run_check() {
  local name=$1
  shift
  echo "→ $name"
  if "$@"; then
    echo "  ✅ PASS"
    ((CHECKS_PASSED++))
  else
    echo "  ❌ FAIL"
    ((CHECKS_FAILED++))
    return 1
  fi
  echo ""
}

# 1. Run existing CI checks
echo "==> Running CI checks"
run_check "Linting (fmt + clippy)" ./scripts/ci-lint.sh
run_check "Tests" ./scripts/ci-test.sh

# 2. Feature matrix
echo "==> Testing feature combinations"
run_check "No default features" cargo build --no-default-features --quiet
run_check "transport_rumqttc" cargo build --features transport_rumqttc --quiet
run_check "transport_mqttac" cargo build --features transport_mqttac --quiet
run_check "All features" cargo build --all-features --quiet

# 3. Documentation
echo "==> Documentation checks"
run_check "Docs build (no warnings)" sh -c 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --quiet'
run_check "Examples compile" cargo build --examples --all-features --quiet

# 4. Package validation
echo "==> Package validation"
run_check "Package builds" cargo package --quiet --allow-dirty
run_check "Dry-run publish" cargo publish --dry-run --quiet --allow-dirty

# Summary
echo "================================================"
echo "Verification Summary"
echo "================================================"
echo "Passed: $CHECKS_PASSED"
echo "Failed: $CHECKS_FAILED"
echo ""

if [ $CHECKS_FAILED -eq 0 ]; then
  echo "✅ ALL CHECKS PASSED - Ready to publish!"
  echo ""
  echo "To publish, run:"
  echo "  cargo publish"
  exit 0
else
  echo "❌ SOME CHECKS FAILED - Please fix before publishing"
  exit 1
fi
