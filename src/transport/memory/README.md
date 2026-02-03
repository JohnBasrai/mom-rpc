# src/transport/memory/

In-memory transport implementation.

This transport provides a **pure in-process broker** that implements the domain-level `Transport` trait without any network, IO, or background tasks.

It is primarily intended for:
- Integration testing
- Local execution
- Validating RPC semantics independent of any real broker

## How It Works

The memory transport simulates a broker using:
- A `HashMap<Subscription, Vec<Sender<Envelope>>>`
- `tokio::sync::mpsc` channels for delivery

There is no polling loop, no callbacks, and no timing-based behavior.

## Semantics

- `subscribe()`:
  - Registers a subscription immediately
  - Once the call returns, any subsequent `publish()` that matches the subscription is deliverable
- `publish()`:
  - Performs a synchronous fan-out to matching subscriptions
  - Delivery order is deterministic within a single process
- Dropping a `SubscriptionHandle` automatically unregisters the subscription

## Guarantees

- No message loss after successful subscription
- No races between subscribe and publish
- No dependency on async runtime scheduling
- Deterministic behavior suitable for tests

## Non-Goals

- No persistence
- No durability
- No wildcard semantics beyond what is explicitly implemented
- No attempt to perfectly emulate MQTT, AMQP, or any other broker

This transport exists to define **reference semantics** for the RPC layer.  If behavior is incorrect here, the bug is in the domain logic, not the transport.
