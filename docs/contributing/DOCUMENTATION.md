# Documentation Standards

This project follows **production-grade documentation standards** for Rust code.

## Required Doc Comments

Use Rust doc comments (`///`) for:

- Public structs and enums (especially `RpcBroker`, `TransportBuilder`, `RpcBrokerBuilder`, message types)
- Public functions (especially `request_to()`, `register()`, `spawn()`)
- Public modules that define architectural boundaries
- Critical system behavior (correlation matching, timeout handling, handler spawning, mode validation)

Doc comments should describe **intent, guarantees, and failure semantics** — not restate what the code obviously does.

## RPC/Messaging-Specific Documentation

For RPC and messaging code, doc comments should explicitly describe:

- **Failure modes** - What happens on timeout, connection loss, transport unavailable?
- **Message flow** - Which part of request/response cycle is this?
- **Concurrency** - Can multiple requests be in-flight? How are handlers executed?
- **Correlation semantics** - How are requests matched to responses?
- **Transport behavior** - Does this depend on specific transport semantics?

## Example: Well-Documented Function

```rust
/// Sends an RPC request to a target node and returns a Future that resolves when the response arrives.
///
/// This method generates a unique correlation ID, publishes the request to the target
/// node's request queue, and awaits a matching response on the response queue.
///
/// # Behavior
///
/// - Correlation ID: UUID v4 (36-byte string format)
/// - Response address: `responses/{node_id}` (subscribed during transport construction)
/// - Request published to: `requests/{target_node_id}`
/// - Concurrent requests: Supported via internal HashMap tracking
///
/// # Errors
///
/// Returns an error if:
/// - `RpcError::InvalidMode` - broker is in `Mode::Server`
/// - Transport is disconnected
/// - Request serialization fails
/// - Publish fails
///
pub async fn request_to<TReq, TResp>(
    &self,
    target_node_id: &str,
    method: &str,
    req: TReq,
) -> Result<TResp>
where
    TReq: Serialize,
    TResp: DeserializeOwned,
{
    // ---
    // implementation
}
```

## Optional (Encouraged) Doc Comments

Doc comments are encouraged for:

- Internal functions with concurrency implications
- Correlation ID generation and matching logic
- Handler registration and execution
- Mode validation logic
- Timeout and error handling

## Not Required

Doc comments are not required for:

- Trivial helpers
- Simple getters or obvious pass-through functions
- Test code (assert messages should be sufficient)

## General Guidance

- Prefer documenting *why* over *how*
- Be explicit about failure behavior and recovery
- Keep comments accurate and up to date
- Avoid over-documenting trivial code
- For RPC patterns, describe correlation and timeout semantics clearly

## Checking Documentation

To verify documentation builds without warnings:

```bash
./scripts/ci-docs.sh
```

Or manually:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

To view generated docs:

```bash
cargo doc --no-deps --all-features --open
```
