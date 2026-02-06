# Contributing to mom-rpc

Thanks for considering contributing!

## Getting Started

New to the project? Start here:

- **[Quick Start Guide](docs/contributing/QUICK_START.md)** - Get up and running in 5 minutes
- **[Local Testing](docs/contributing/LOCAL_TESTING.md)** - Test before committing

## Before Submitting a PR

Run these commands:

```bash
./scripts/ci-lint.sh && ./scripts/ci-test.sh
```

Or for comprehensive testing:

```bash
./scripts/local-test.sh
```

**PR Checklist:**
- Keep commits focused and descriptive
- Add tests for new features
- Update `CHANGELOG.md` under [Unreleased] if behavior changes
- Verify all CI checks pass locally

We follow [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and [Semantic Versioning](https://semver.org/).

## Documentation

- **[Quick Start](docs/contributing/QUICK_START.md)** - Examples and basic usage
- **[Local Testing](docs/contributing/LOCAL_TESTING.md)** - Running CI locally
- **[Code Style](docs/contributing/CODE_STYLE.md)** - Formatting conventions
- **[Documentation Standards](docs/contributing/DOCUMENTATION.md)** - Doc comment requirements
- **[Architecture Guidelines](docs/contributing/ARCHITECTURE.md)** - EMBP and module structure
- **[Testing Strategy](docs/contributing/TESTING.md)** - When and how to add tests

## Quick Reference

- **Format code:** `./scripts/ci-lint.sh`
- **Run tests:** `./scripts/ci-test.sh`
- **Full CI check:** `./scripts/local-test.sh`
- **Documentation:** `cargo doc --open`

## Questions?

Open an issue or discussion on GitHub.
