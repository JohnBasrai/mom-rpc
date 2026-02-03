# src/transport/

This directory contains **concrete transport implementations** for the domain-level `Transport` trait defined in `src/domain/transport.rs`.

## Design Goals

- Provide pluggable backends (MQTT, RabbitMQ, in-memory, etc.)
- Keep all protocol- and broker-specific concerns **out of the domain layer**
- Allow `client/` and `server/` code to depend only on domain abstractions
- Support deterministic testing via non-network transports

## Architectural Rules

- Code in `domain/` must not depend on anything in `transport/`
- Transport implementations must not leak protocol-specific types upward
- All transports are constructed via factory functions and returned as `TransportPtr`
- Feature flags are used to gate optional transports

## Directory Layout

- `memory/`
  - In-process, deterministic transport for testing and local execution
- `mqtt_async_client/`
  - MQTT transport built on top of the `mqtt-async-client` crate
- (future)
  - `rabbit_mq/`
  - `nats/`
  - etc.

Each transport subdirectory may contain additional documentation describing backend-specific semantics, constraints, or trade-offs.
