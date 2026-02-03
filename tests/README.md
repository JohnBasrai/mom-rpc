# Integration tests for transport behavior.

These tests define the observable contract of the transport layer as used by higher-level components.

## Intentional Non-Goals

The following behaviors are intentionally NOT tested or guaranteed by the transport layer:

- Message replay or buffering for late subscribers
- Exactly-once delivery
- Single-consumer (queue-like) semantics
- Deduplication based on correlation identifiers
- Durability or persistence across restarts

These concerns are expected to be handled by higher layers (e.g. RPC logic, application policy) or by transport-specific configuration outside the scope of this abstraction.
