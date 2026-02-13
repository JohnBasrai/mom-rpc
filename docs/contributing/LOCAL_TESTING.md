# Local Testing Guide

Run CI checks locally to catch issues before pushing.

## Quick Check (Before Commit)

Run the standard CI checks locally:

```bash
./scripts/ci-lint.sh   # Format and clippy (~30s)
./scripts/ci-test.sh   # Run tests with default features (~1m)
./scripts/ci-docs.sh   # Check documentation (~1m)
```

## Comprehensive Check (Before Push)

Test all feature combinations to match CI:

```bash
./scripts/local-test.sh
```

This runs the complete test suite that mirrors CI:
- Linting (format + clippy)
- Feature matrix (5 combinations)
- Documentation validation
- Example compilation

**Time:** ~5-10 minutes, catches issues before CI runs.

## Individual Feature Testing

To test a specific feature combination:

```bash
# Test with rumqttc
CI_FEATURE=transport_rumqttc ./scripts/ci-test.sh

# Test with dust_ddc
CI_FEATURE=transport_dustddc ./scripts/ci-test.sh

# Test without default features
CI_FEATURE=no-default-features ./scripts/ci-test.sh

# Test with all features
CI_FEATURE=all-features ./scripts/ci-test.sh
```

## Using act (Local GitHub Actions)

To run the actual GitHub Actions workflow locally:

```bash
# Install act
brew install act  # macOS
# or: curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash

# Run full CI locally
act pull_request

# Run specific job
act -j test
act -j docs
```

**Note:** `act` requires Docker and takes longer than the shell scripts.

## What to Run When

| Situation | Command | Time |
|-----------|---------|------|
| **Quick check** | `./scripts/ci-lint.sh` | ~30s |
| **Before commit** | `./scripts/ci-lint.sh && ./scripts/ci-test.sh` | ~1m |
| **Before push** | `./scripts/local-test.sh` | ~5m |
| **Before release** | `./scripts/pre-publish.sh` | ~10m |
| **Debug CI failure** | `act` | ~10m |

## CI Environment Variables

Our CI scripts respect these environment variables:

- `CI_FEATURE` - Which feature to test (default, no-default-features, all-features, etc.)
- `CI_COVERAGE` - Set to "1" to run coverage tests
- `CI` - Set to "true" to indicate running in CI

Example:
```bash
CI_FEATURE=all-features CI=true ./scripts/ci-test.sh
```

## Troubleshooting

### Test Failures

If tests fail locally but pass in CI (or vice versa):

1. Check feature flags match: `cargo test --all-features`
2. Clear build cache: `cargo clean`
3. Verify Rust version matches CI: `rustc --version`

### Format Failures

If formatting check fails:

```bash
cargo fmt
./scripts/ci-lint.sh
```

### Clippy Warnings

To see all clippy warnings:

```bash
cargo clippy --all-targets --all-features
```
