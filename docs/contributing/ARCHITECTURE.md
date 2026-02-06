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
│   └── pending.rs    # In-flight request tracking
├── server/           # RPC server
│   ├── mod.rs        # Gateway: RpcServer
│   └── handler.rs    # Handler execution
└── transport/        # Transport implementations
    ├── mod.rs        # Transport factory
    ├── memory/       # In-memory transport (always available)
    │   ├── mod.rs
    │   └── transport.rs
    └── rumqttc/      # MQTT transport (optional feature)
        ├── mod.rs
        └── transport.rs
```

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
pub use memory::create_memory_transport;
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
- Memory transport (testing)
- MQTT transports (production)
- Each transport is a separate module

## Adding New Modules

When adding a new module:

1. Create module directory with `mod.rs`
2. Define public API in `mod.rs`
3. Keep implementation details in separate files
4. Export only what's needed via `mod.rs`
5. Document module purpose in `mod.rs`

## Adding New Transport

When adding a new transport backend:

1. Create `src/transport/new_backend/`
2. Implement `Transport` trait in `transport.rs`
3. Export factory function in `mod.rs`
4. Gate with feature flag
5. Update `src/transport/mod.rs` factory

Example:

```rust
// src/transport/new_backend/mod.rs
mod transport;
pub use transport::create_new_backend_transport;

// src/transport/mod.rs
#[cfg(feature = "transport_new_backend")]
pub use new_backend::create_new_backend_transport;
```

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
