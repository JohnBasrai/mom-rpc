# Documentation Standards

This project follows **production-grade documentation standards** for Rust code.

## Required Doc Comments

Use Rust doc comments (`///`) for:

- Public structs and enums (especially `RpcClient`, `RpcServer`, message types)
- Public functions (especially `request()`, `handle()`, `run()`)
- Public modules that define architectural boundaries
- Critical system behavior (correlation matching, timeout handling, handler spawning)

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
/// Sends an RPC request and returns a Future that resolves when the response arrives.
///
/// This method generates a unique correlation ID, subscribes to the response address
/// if not already subscribed, and publishes the request to the target service.
///
/// # Behavior
///
/// - Correlation ID: UUID v4 (36-byte string format)
/// - Response address: `{service_name}/responses/{client_id}`
/// - Request published to: `{service_name}/{method}`
/// - Concurrent requests: Supported via internal HashMap tracking
///
/// # Errors
///
/// Returns an error if:
/// - Transport is disconnected
/// - Request serialization fails
/// - Publish fails
///
/// # Timeouts
///
/// Timeouts are handled at the application layer. Consider using
/// `tokio::time::timeout()` when awaiting the returned future.
pub async fn request<Req, Resp>(&self, service: &str, method: &str, payload: &Req) 
    -> Result<Resp>
where
    Req: Serialize,
    Resp: DeserializeOwned,
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
