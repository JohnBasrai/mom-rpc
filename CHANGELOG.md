# Changelog

All notable changes to this project will be documented in this file.

This project follows a design-first, architecture-driven development model.
Early versions may include intentional refactors as semantics are clarified.


## [0.7.0] - 2026-02-13

### Added

- **DDS transport support** via `dust_dds` library (#38)
  - Brokerless peer-to-peer communication using RTPS protocol
  - WaitSet-based discovery synchronization to prevent timing races
  - Actor-based concurrency model with async API integration
  - Enable with `features = ["transport_dust_dds"]`
- `wait_for_matched_reader()` - discovery synchronization using `StatusCondition` and `WaitSetAsync`
- Manual integration test script: `scripts/manual-tests/dds.sh`

### Changed

- **Breaking**: Migrated from `log` to `tracing` for diagnostic output
  - Users should update to `tracing-subscriber` instead of `env_logger`, see examples
  - Logging macros refactored to crate-level (lib.rs)
  - Stderr fallback for error logging when logging feature is disabled
- **Dependencies**: Added `dust_dds = "0.14"` for DDS transport implementation
- **Dependencies**: Replaced `log` with `tracing` for logging

### Internal

- Enforce clippy `no-unwrap` and `no-panic` rules in production code
- Improved error handling: removed infallible conversions, eliminated `expect()`/`panic()` usage
- DDS `DdsEnvelope` conversions changed from `From` to `TryFrom` for proper error handling

### Documentation

- Added `transport/dds/README.md` with library selection rationale and technical implementation notes
- Updated `scripts/manual-tests/README.md` with DDS testing instructions

### Known Limitations

- DDS transport is functional but not stress-tested for concurrent clients or high-burst scenarios
- See [GitHub issue #40] for production hardening roadmap

## [0.6.3] - unreleased

### Added

- Dual-license crate under MIT OR Apache-2.0
- Readme: Client & Server examples changed to return mpm_Rpc::Error instead of anyhow::Error

### Fixed

- `sensor_server` example was using `spawn()` instead of `run()`, which is the correct method for a standalone server process (#35)

### Tests

- Added `test_run_blocks_until_shutdown` integration test to verify `run()` blocks until `shutdown()` is called (#35)

## [0.6.2] - 2026-02-10

### Fixed

- Quick Start example in `lib.rs` was stale math example — replaced with sensor example
- `request_with_timeout` doc example missing `TemperatureUnit` enum definition
- Feature flag name `transport_amqp` → `transport_lapin` in `lib.rs` doc comment

### Changed

- Renamed `scripts/manual-tests/rabbitmq.sh` → `amqp.sh` (protocol-first naming convention)
- Updated ARCHITECTURE.md: transport derivation lineage, pinned EMBP link, removed misleading multi-transport priority note
- Feature flags table: clearer "Default Enable" column with ✅/❌ indicators
- RabbitMQ expected test output updated to match actual sensor output
- Security section now mentions both `rumqttc` and `lapin` transports
- Version references in README updated from `0.4` to `0.x`

## [0.6.1] - 2026-02-10

### Changed
- Replaced math-based examples with sensor-based examples to better reflect realistic RPC usage (finite domains, device-style request/response).
- Updated manual MQTT and RabbitMQ test scripts to exercise the sensor examples.

### Updated
- Updated lapin to v4.0.0; adapted AMQP transport to use owned string parameters per new API.

### Notes
- No public APIs were changed.

## [0.6.0] - 2026-02-09

### Added

- **MQTT manual integration test** - `scripts/manual-tests/mqtt.sh` for testing `transport_rumqttc` against real Mosquitto broker
- **Timeout convenience method** - `RpcClient::request_with_timeout()` for simpler timeout handling

### Changed (BREAKING)

- **Removed `PublishOptions`** - Simplified `Transport::publish()` to remove unused options parameter
  - RPC semantics always use non-durable delivery with no TTL
  - **Breaking:** Custom `Transport` implementations must update `publish()` signature
  - **Non-breaking:** Public API users (`RpcClient`, `RpcServer`) are unaffected

### Documentation

- Updated timeout handling section in README to feature `request_with_timeout` method
- Added comprehensive tests for timeout functionality

---

## [0.5.3] - Unreleased

### Changed
- Refactored module structure: moved implementation code from `mod.rs` to dedicated module files
- Refactored examples: extract math types to common/math_types module
- Add MSRV validation job to CI (1.75.0, runs on main only)
- Added module-level documentation for client, server, and handler components
- Minor README formatting improvements

## [0.5.2] - 2026-02-09

### Fixed

- Add missing blank line in server broker code fence example
  - Markdown requires blank line between HTML tags and code fences

## [0.5.1] - 2026-02-08

### Changed

- Update crate description to include AMQP

### Fixed

- Fix client example to use reference for broker_uri (`&broker_uri` instead of `broker_uri`)
  - Ensures consistency with server example

### Documentation

- Clarify example titles to distinguish memory vs broker-based patterns
  - "View complete memory example" vs "View complete server/client with broker example"
- Update README broker examples to match actual example files
  - Add `env_logger::init()` calls
  - Add `BROKER_URI` env var support for testing different transports
- Update `transport_lapin` status from "Production Ready" to "Available"
  - More honest assessment for newly released transport

## [0.5.0] - 2026-02-08

### Added

- **AMQP transport via lapin** for RabbitMQ and AMQP 0-9-1 brokers
  - Actor-based concurrency with ephemeral queue semantics
  - Enable via `transport_lapin` feature flag
- Manual integration test scripts in `scripts/manual-tests/`
  - `rabbitmq.sh` for automated AMQP testing

### Changed

- **BREAKING:** RpcConfig API updated for multi-transport support
  - `broker_addr: String` → `transport_uri: Option<String>`
  - Added optional `request_queue_name` and `response_queue_name` (AMQP-specific)
  - Added builder methods: `with_request_queue_name()`, `with_response_queue_name()`

- **BREAKING:** Feature renamed for consistency
  - Features now named after library, not protocol
  - Enables multiple implementations per protocol

- Transport directory restructured to protocol → library hierarchy
  - `transport/memory.rs` (flat)
  - `transport/mqtt/rumqttc.rs` (MQTT via rumqttc)
  - `transport/amqp/lapin.rs` (AMQP via lapin)
  - EMBP gateways at protocol level

### Dependencies

- Added `lapin = "2"` (AMQP client, optional)
- Added `futures-lite = "2"` (for lapin streams, optional)

### Documentation

- Updated architecture.md with AMQP transport details and directory structure
- Updated contributing/ARCHITECTURE.md with transport addition guide
- Added README section for RabbitMQ testing
- Examples now support `BROKER_URI` env var for multi-transport testing

## [0.4.0] - 2026-02-07

### Changed

- **BREAKING:** Remove SubscribeOptions struct (unused durable field)
  - Simplify Transport::subscribe() signature
  - Update all implementations and callers
  - Document that subscriptions remain active until close()

- **BREAKING:** Renamed `Error` enum to `RpcError` to avoid confusion with `std::error::Error` (#6)
  - Update imports: `use mom_rpc::Error` → `use mom_rpc::RpcError`
  - The `Result<T>` type alias remains unchanged and now uses `RpcError` internally

### Removed

  - Remove deprecated mqtt-async-client transport (#12)

### Improved

- Enhanced `TransportPtr` documentation to clarify Arc semantics and connection sharing
- Added logging configuration guide to README explaining default levels and how to control verbosity


## [0.3.0] - 2026-02-06

### Added

* Optional **rumqttc-based MQTT transport** behind the `transport_rumqttc` feature flag
* Actor-based MQTT transport implementation with single `EventLoop` ownership
* Lazy broker connection initiated on first event loop poll
* SUBACK-confirmed subscription registration for brokered transports
* Topic-based fanout semantics aligned with in-process memory transport behavior

### Changed

* Extended transport selection logic to prefer `transport_rumqttc` when enabled
* Clarified transport precedence and fallback behavior in `create_transport()`

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
