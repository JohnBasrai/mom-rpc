# Architecture Guidelines

This project uses the [Explicit Module Boundary Pattern (EMBP)](https://github.com/JohnBasrai/architecture-patterns/blob/main/rust/embp.md) for module organization.

**Please review the EMBP documentation before making structural changes.**

## Key EMBP Principles

- Each module's public API is defined in its `mod.rs` gateway file
- Sibling modules import from each other using `super::`
- External modules import through `crate::module::`
- Never bypass module gateways with deep imports

## mom-rpc Module Structure

```
src/
├── lib.rs            # Public API exports
├── error.rs          # Error types
├── rpc_config.rs     # Configuration
├── correlation.rs    # Correlation ID handling
├── domain/           # Domain types and traits
│   ├── mod.rs        # Gateway: Transport trait, core types
│   └── transport.rs  # Transport abstraction
├── client/           # RPC client
│   ├── mod.rs        # Gateway: RpcClient
│   ├── pending.rs    # In-flight request tracking
│   └── rpc_client.rs # RpcClient implementation
├── server/           # RPC server
│   ├── mod.rs        # Gateway: RpcServer
│   ├── rpc_server.rs # RpcServer implementation
│   └── handler.rs    # Handler execution
└── transport/        # Transport implementations
    ├── mod.rs        # Top-level gateway, exports all transports
    ├── memory.rs     # In-memory transport (always available)
    ├── mqtt/         # MQTT protocol transports
    │   ├── mod.rs    # Protocol gateway (EMBP)
    │   └── rumqttc.rs # MQTT via rumqttc library
    └── amqp/         # AMQP protocol transports
        ├── mod.rs    # Protocol gateway (EMBP)
        └── lapin.rs  # AMQP via lapin library
```

## Transport Organization

Transports are organized by **protocol → library** hierarchy:

- **Protocol level** (`mqtt/`, `amqp/`, etc.) - Contains gateway `mod.rs` and library implementations
- **Library level** (`rumqttc.rs`, `lapin.rs`) - Single-file transport implementations
- **Memory transport** (`memory.rs`) - Flat at top level (signals special/default status)

**Why this structure?**
- Allows multiple implementations per protocol
- Clear separation between protocols
- Feature names follow library convention (`transport_rumqttc`, `transport_lapin`)
- EMBP maintained at protocol level

## Import Guidelines

### From Parent Module

```rust
// In src/client/pending.rs
use super::RpcClient;  // From src/client/mod.rs
```

### From Sibling Module

```rust
// In src/client/mod.rs
use super::correlation::CorrelationId;  // From src/correlation.rs
```

### From Child Module

```rust
// In src/transport/mod.rs
mod memory;
pub use memory::create_transport as create_memory_transport;
```

### From Domain

```rust
// Any module importing domain types
use crate::domain::{Transport, Envelope, Address};
```

## Module Responsibilities

### domain/
- Defines core abstractions: `Transport` trait
- Core domain types: `Envelope`, `Address`, `Subscription`
- No business logic, only contracts

### client/
- `RpcClient` - Send requests, receive responses
- Manages correlation IDs and pending requests
- Handles response routing

### server/
- `RpcServer` - Receive requests, dispatch to handlers
- Handler registration and execution
- Response publishing

### transport/
- Transport implementations
- Memory transport (testing/default)
- Protocol-based transports (MQTT, AMQP, etc.)
- Each protocol may have multiple library implementations

## Adding New Modules

When adding a new module:

1. Create module directory with `mod.rs`
2. Define public API in `mod.rs`
3. Keep implementation details in separate files
4. Export only what's needed via `mod.rs`
5. Document module purpose in `mod.rs`

## Adding New Transport

Transports are organized by **protocol → library**. When adding a new transport:

### Adding First Implementation for a New Protocol

1. Create protocol directory: `src/transport/kafka/`
2. Create protocol gateway: `src/transport/kafka/mod.rs`
3. Create library implementation: `src/transport/kafka/rdkafka.rs`
4. Add feature flag: `transport_rdkafka = ["rdkafka"]`
5. Update top-level gateway: `src/transport/mod.rs`

Example protocol gateway (`src/transport/kafka/mod.rs`):

```rust
//! Kafka protocol transports.
//!
//! This module contains transport implementations for Kafka brokers.
//! Currently supports:
//! - rdkafka - Kafka via rdkafka library

#[cfg(feature = "transport_rdkafka")]
mod rdkafka;

#[cfg(feature = "transport_rdkafka")]
pub use rdkafka::create_transport as create_rdkafka_transport;
```

Example library implementation (`src/transport/kafka/rdkafka.rs`):

```rust
//! Kafka transport implementation using rdkafka.

use crate::{Result, RpcConfig, Transport, TransportPtr};

pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // Implementation
}
```

Update top-level gateway (`src/transport/mod.rs`):

```rust
#[cfg(feature = "transport_rdkafka")]
mod kafka;

#[cfg(feature = "transport_rdkafka")]
pub use kafka::create_rdkafka_transport;
```

### Adding Second Implementation for Existing Protocol

If a protocol directory already exists (e.g., `mqtt/`), just add the new library:

1. Create library implementation: `src/transport/mqtt/paho.rs`
2. Add feature flag: `transport_paho = ["paho-mqtt"]`
3. Update protocol gateway: `src/transport/mqtt/mod.rs`

Example update to protocol gateway:

```rust
//! MQTT protocol transports.

#[cfg(feature = "transport_rumqttc")]
mod rumqttc;

#[cfg(feature = "transport_paho")]
mod paho;

#[cfg(feature = "transport_rumqttc")]
pub use rumqttc::create_transport as create_rumqttc_transport;

#[cfg(feature = "transport_paho")]
pub use paho::create_transport as create_paho_transport;
```

Update top-level gateway to export both:

```rust
#[cfg(feature = "transport_rumqttc")]
pub use mqtt::create_rumqttc_transport;

#[cfg(feature = "transport_paho")]
pub use mqtt::create_paho_transport;
```

### Feature Naming Convention

Features are named after the **library**, not the protocol:

- ✅ `transport_rumqttc` (library name)
- ✅ `transport_lapin` (library name)
- ✅ `transport_paho` (library name)
- ❌ `transport_mqtt` (too generic)
- ❌ `transport_amqp` (too generic)

This allows multiple implementations per protocol without naming conflicts.

## Testing Module Structure

Each module can have its own tests:

```rust
#[cfg(test)]
mod tests {
    // ---
    use super::*;
    
    #[test]
    fn test_something() {
        // test
    }
}
```

Integration tests go in `tests/` directory.
