# Architecture Guidelines

This project uses the [Explicit Module Boundary Pattern (EMBP)](https://github.com/JohnBasrai/architecture-patterns/blob/350e668f/rust/embp.md) for module organization.

**Please review the EMBP documentation before making structural changes.**

## Key EMBP Principles

- Each module's public API is defined in its `mod.rs` gateway file
- Sibling modules import from each other using `super::`
- External modules import through `crate::module::`
- Never bypass module gateways with deep imports

## mom-rpc Module Structure

```
src
├── lib.rs               # Public API exports
├── broker_builder.rs    # RPC broker builder
├── broker_mode.rs       # Broker mode enumeration
├── broker.rs            # FullDuplex RPC broker
├── correlation.rs       # Correlation ID handling
├── error.rs             # Errors surfaced by the RPC layer
├── macros.rs            # Global shared logging macros
├── retry.rs             # Retry configuration and exponential backoff logic
├── transport_builder.rs # Transport builder for creating transport instances
├── domain               # Domain types and traits
│   ├── mod.rs           # Gateway: Transport trait, core types
│   └── transport.rs     # Transport abstraction
└── transport            # Transport implementations
    ├── mod.rs           # Protocol gateway (EMBP)
    ├── amqp             # AMQP protocol transports
    │   ├── lapin.rs     # AMQP via lapin library
    │   └── mod.rs       # Protocol gateway (EMBP)
    ├── dds              # DDS protocol transports
    │   ├── mod.rs       # Protocol gateway (EMBP)
    │   ├── dust_dds.rs  # DDS via dust_dds library
    │   └── README.md    # DDS design notes
    ├── memory.rs        # In-memory transport (always available)
    ├── mqtt             # MQTT protocol transports
    │   ├── mod.rs       # Protocol gateway (EMBP)
    │   └── rumqttc.rs   # MQTT via rumqttc library
    └── redis            # Redis protocol transports
        ├── mod.rs       # Protocol gateway (EMBP)
        ├── redis.rs     # Redis Pub/Sub via redis library
        └── README.md    # Redis transport design notes

```

## Transport Organization

Transports are organized by **protocol → library** hierarchy:

- **Protocol level** (`mqtt/`, `amqp/`, etc.) - Contains gateway `mod.rs` and library implementations
- **Library level** (`rumqttc.rs`, `lapin.rs`) - Single-file transport implementations
- **Memory transport** (`memory.rs`) - Flat at top level (signals special/default status)

**Why this structure?**
- Allows multiple implementations per protocol
- Clear separation between protocols
- Feature names follow library convention (`transport_rumqttc`, `transport_lapin` ⋯ )
- EMBP maintained at protocol level

## Import Guidelines

For import guidelines see the _EMBP documentation_ linked at the top of this document.

## Module Responsibilities

### domain/
- Defines core abstractions: `Transport` trait
- Core domain types: `Envelope`, `Address`, `Subscription`
- No business logic, only contracts

### broker/
- `RpcBroker` - Unified type for client, server, and full-duplex modes
- Mode inference from transport queue configuration
- Handler registration and execution (server/full-duplex)
- Request/response correlation (client/full-duplex)
- Runtime mode validation

### transport_builder/
- `TransportBuilder` - Configures and constructs transport instances
- Determines broker mode from queue subscriptions
- Delegates actual transport creation to `transport/`

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

To add a new transport, use `src/transport/redis/` as the reference implementation. It is the most recent transport added and reflects current API patterns. Use the protocol → library directory structure and feature naming conventions described above in [Module Structure](#mom-rpc-module-structure).
