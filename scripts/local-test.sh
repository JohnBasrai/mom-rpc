#!/usr/bin/env bash
set -euo pipefail

echo "================================================"
echo "Local Testing Suite (mirrors CI)"
echo "================================================"
echo ""

# Track results
FAILED_TESTS=()

run_test() {
    local name=$1
    local feature=$2
    
    echo "→ Testing: $name"
    if CI_FEATURE="$feature" ./scripts/ci-test.sh; then
        echo "  ✅ PASS"
    else
        echo "  ❌ FAIL"
        FAILED_TESTS+=("$name")
    fi
    echo ""
}

# 1. Linting
echo "==> Linting"
if ./scripts/ci-lint.sh; then
    echo "✅ Lint PASS"
else
    echo "❌ Lint FAIL"
    FAILED_TESTS+=("lint")
fi
echo ""

# 2. Feature matrix testing
echo "==> Feature Matrix"
run_test "default features" "default"
run_test "rumqttc" "transport_rumqttc"
run_test "lapin" "transport_lapin"
run_test "dust_dds" "transport_dust_dds"
run_test "no default features" "no-default-features"
run_test "all features" "all-features"

# 3. Documentation
echo "==> Documentation"
if ./scripts/ci-docs.sh; then
    echo "✅ Docs PASS"
else
    echo "❌ Docs FAIL"
    FAILED_TESTS+=("docs")
fi
echo ""

# Summary
echo "================================================"
echo "Summary"
echo "================================================"

if [ ${#FAILED_TESTS[@]} -eq 0 ]; then
    echo "✅ ALL TESTS PASSED"
    echo ""
    echo "Ready to push! Your changes will pass CI."
    exit 0
else
    echo "❌ ${#FAILED_TESTS[@]} TEST(S) FAILED:"
    for test in "${FAILED_TESTS[@]}"; do
        echo "  - $test"
    done
    echo ""
    echo "Fix these before pushing."
    exit 1
fi
