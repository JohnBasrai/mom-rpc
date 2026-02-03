# src/transport/mqtt_async_client/

MQTT transport implementation built on top of the `mqtt-async-client` crate.

This transport adapts an MQTT client to the domain-level `Transport` trait without exposing MQTT-specific concepts (topics, QoS, callbacks, etc.)  to higher layers.

## Design Intent

- Provide a production-capable MQTT backend
- Hide all client lifecycle and callback mechanics
- Present a clean, async-native API consistent with other transports

## Key Characteristics

- Uses `mqtt-async-client` rather than `rumqttc`
- Relies on the client library to manage IO and background processing
- Adapts incoming messages into domain `Envelope` values
- Translates domain `Address` and `Subscription` into MQTT topics and filters

## Constraints and Trade-offs

- Message ordering and delivery guarantees are ultimately determined by the MQTT broker and client library
- Subscription readiness semantics depend on the underlying client behavior
- Retained messages, QoS levels, and session persistence are backend concerns
  and may not map perfectly to the domain model

## Rationale

This implementation exists to support real MQTT-based deployments while keeping the core RPC and domain logic transport-agnostic.

All MQTT-specific quirks and limitations should be documented and handled here rather than leaking upward into the domain or client layers.
