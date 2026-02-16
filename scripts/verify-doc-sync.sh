#!/usr/bin/env bash
set -euo pipefail

# Get version from Cargo.toml
cargo_version=$(awk -F'"' '/^version/ {print $2; exit}' Cargo.toml)
# Extract minor version (0.7.1 -> 0.7)
expected_version=$(echo "$cargo_version" | cut -d. -f1-2)

echo
echo "==> Verifying doc sync for version $cargo_version"

get_lines() {
    grep -E -i -A 8 "\| *Transport *\|" $1
}

get_tx_count() {
    get_lines $1 | grep transport_ | wc -l
}


readme_count=$(get_tx_count README.md)
librs_count=$(get_tx_count src/lib.rs)

if [[ "${readme_count}" != "${librs_count}" ]] ; then
    echo "ERROR: Feature flags tables don't match between README.md and src/lib.rs"

    echo "README.md:"
    get_lines "README.md"

    echo "src/lib.rs:"
    get_lines "src/lib.rs"
    exit 1
fi
readme_flags=$(get_lines README.md | grep -o 'transport_[a-z_]*' | sort)
librs_flags=$(get_lines src/lib.rs | grep -o 'transport_[a-z_]*' | sort)

if [[ "${readme_flags}" != "${librs_flags}" ]] ; then
    echo "ERROR: Feature flag names don't match"
    echo "README: ${readme_flags}"
    echo "lib.rs: ${librs_flags}"
    exit 1
fi

# Check version consistency in examples
echo
echo "  ==> Checking version consistency in examples"

echo "    Cargo.toml version: $cargo_version"
echo "    Expected in docs:   $expected_version"

readme_versions=$(grep -o 'mom-rpc = .*version = "[^"]*"' README.md | \
    sed -E 's/.*version = "([^"]*)".*/\1/' | sort -u)

librs_versions=$(grep -o 'mom-rpc = .*version = "[^"]*"' src/lib.rs | \
    sed -E 's/.*version = "([^"]*)".*/\1/' | sort -u)


# Check all versions match expected
if [[ "$readme_versions" != "$expected_version" ]]; then
    echo "ERROR: README.md examples use version: $readme_versions"
    echo "       Expected: $expected_version (from Cargo.toml)"
    exit 1
fi

if [[ "$librs_versions" != "$expected_version" ]]; then
    echo "ERROR: src/lib.rs examples use version: $librs_versions"
    echo "       Expected: $expected_version (from Cargo.toml)"
    exit 1
fi

echo "  ✓ Release link OK"

# -------------------------------------------------------------------
# Verify README release notes link matches Cargo.toml full version
# -------------------------------------------------------------------
echo
echo "  ==> Checking README release versioned link"

EXPECTED_DOCS="https://docs.rs/mom-rpc"
EXPECTED_RELEASE="https://github.com/JohnBasrai/mom-rpc/releases/tag/v${cargo_version}"

for expected in "$EXPECTED_DOCS" "$EXPECTED_RELEASE"; do
    if ! grep -q "$expected" README.md; then
        echo "ERROR: README.md missing expected link:"
        echo "       $expected"
        exit 1
    fi
done
echo "  ✓ Versioned links OK"

echo
echo "  ==> Checking README SLOC table"

cargo_version=$(awk -F'"' '/^version/ {print $2; exit}' Cargo.toml)
readme_version=$(grep -oP 'As of v\K[\d.]+(?=\.)' README.md | head -1)

if [ "$cargo_version" != "$readme_version" ]; then
    echo "ERROR: SLOC table shows v$readme_version but Cargo.toml has v$cargo_version"
    exit 1
fi
echo "  ✓ SLOC table OK"
echo
echo "✓ All documentation checks passed for v$cargo_version"
