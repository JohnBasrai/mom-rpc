# Changelog

All notable changes to this project will be documented in this file.

This project follows a design-first, architecture-driven development model.
Early versions may include intentional refactors as semantics are clarified.

---

## [0.2.0] – 2026-02-03

### Fixed
- Corrected in-process memory transport semantics to require shared state
- Eliminated client/server isolation bugs caused by per-transport subscription maps
- Prevented unintended self-delivery behavior in loopback scenarios

### Added
- `math_memory` example demonstrating correct in-process client/server usage
- Clear separation between in-process (memory) and brokered transport examples

### Changed
- Updated `math_client` and `math_server` examples to compile against current APIs
- Refined transport abstractions to better reflect real broker semantics
- Aligned integration tests with corrected transport model

### Removed
- Removed obsolete transport runner no longer compatible with current architecture

---

## [0.1.0] – 2026-02-03

### Added
- Initial transport-agnostic asynchronous RPC architecture
- Envelope-based request/response framing with explicit correlation IDs
- `RpcClient` and `RpcServer` abstractions independent of transport addressing
- In-memory reference transport for development and testing
- Method-based server dispatch model
- Typed, crate-scoped error handling
- Initial architecture documentation and examples
