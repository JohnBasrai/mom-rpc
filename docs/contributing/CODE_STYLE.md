# Code Style Guide

This project uses `rustfmt` for consistent code formatting.

## Visual Separators

Since `rustfmt` removes blank lines at the start of impl blocks, function bodies, and module blocks, we use comment separators `// ---` for visual clarity.

### When to Use Separators

Use `// ---` in:
- Use blocks
- Impl blocks
- Struct definitions
- Function bodies (between logical steps)
- Test modules

### Examples

**Use blocks:**
```rust
use crate::{
    // ---
    Address,
    Envelope,
    Result,
};
```

**Struct definitions:**
```rust
struct MemoryTransport {
    // ---
    subscriptions: RwLock<HashMap<Subscription, Vec<mpsc::Sender<Envelope>>>>,
    transport_id: String,
}
```

**Impl blocks:**
```rust
#[async_trait::async_trait]
impl Transport for MemoryTransport {
    // ---
    fn transport_id(&self) -> &str {
        self.transport_id.as_str()
    }

    async fn publish(&self, env: Envelope) -> Result<()> {
        // ---
        let subs = self.subscriptions.read().await;

        for (sub, senders) in subs.iter() {
            if sub.0 == env.address.0 {
                for sender in senders {
                    sender.send(env.clone()).await?;
                }
            }
        }

        Ok(())
    }
}
```

**Function definitions:**
```rust
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    let transport_id = config.transport_id.clone();
    
    let transport = MemoryTransport {
        // ---
        transport_id,
        subscriptions: RwLock::new(HashMap::new()),
    };

    Ok(Arc::new(transport))
}
```

**Test modules:**
```rust
#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[tokio::test]
    async fn test_subscription_delivery() {
        // ---
        // test body
    }
}
```

## Style Guidelines

1. Use `// ---` for visual separation after opening braces
2. Place separator before the first meaningful line
3. Use between meaningful logical steps within function bodies
4. **Do NOT** use separators between fields in struct literals
5. Keep separators consistent across the codebase

## When NOT to Use Separators

**Don't use in struct literals:**
```rust
// CORRECT - no separator
let config = RpcConfig {
    transport_id: id,
    broker_url: url,
};

// INCORRECT - don't do this
let config = RpcConfig {
    // ---
    transport_id: id,
    broker_url: url,
};
```

## Running Format Check

```bash
./scripts/ci-lint.sh
```

Or manually:

```bash
cargo fmt --all -- --check
```

To auto-format:

```bash
cargo fmt
```
